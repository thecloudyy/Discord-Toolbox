use gloo_timers::future::TimeoutFuture;
use leptos::ev::MouseEvent;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::icons as icon;
use crate::types::{Config, ConfigExt, *};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Dashboard,
    Badges,
    HypeSquad,
    Logs,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Badges => "Game badges",
            Tab::HypeSquad => "HypeSquad",
            Tab::Logs => "Logs",
        }
    }
}

fn event_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| {
            js_sys::Reflect::get(&target, &wasm_bindgen::JsValue::from_str("value")).ok()
        })
        .and_then(|value| value.as_string())
        .unwrap_or_default()
}

fn event_checked(event: &web_sys::Event) -> bool {
    event
        .target()
        .and_then(|target| {
            js_sys::Reflect::get(&target, &wasm_bindgen::JsValue::from_str("checked")).ok()
        })
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn format_count(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn truncate_log(text: &mut String) {
    const LIMIT: usize = 60_000;
    if text.len() > LIMIT {
        let cut = text.len() - LIMIT;
        let boundary = text[cut..].find('\n').map(|index| cut + index + 1).unwrap_or(cut);
        text.drain(..boundary);
    }
}

fn append_log(target: RwSignal<String>, chunk: &str) {
    if chunk.is_empty() {
        return;
    }
    target.update(|text| {
        text.push_str(chunk);
        truncate_log(text);
    });
}

fn save_config(cfg: RwSignal<Config>) {
    let entries = cfg.get_untracked();
    spawn_local(async move {
        api::write_env(entries).await;
    });
}

fn set_value(cfg: RwSignal<Config>, key: &'static str, value: String) {
    cfg.update(|config| {
        config.insert(key.to_string(), value);
    });
}

fn choose_account(account: Account, cfg: RwSignal<Config>, profile: RwSignal<Profile>, selected: RwSignal<bool>) {
    profile.set(account.profile);
    selected.set(true);
    set_value(cfg, "DISCORD_TOKEN", account.token.clone());
    save_config(cfg);
}

#[component]
pub fn App() -> impl IntoView {
    let tab = RwSignal::new(Tab::Dashboard);
    let cfg = RwSignal::new(Config::new());
    let profile = RwSignal::new(Profile::default());
    let tokens = RwSignal::new(Vec::<Account>::new());
    let notes = RwSignal::new(Vec::<String>::new());
    let detecting = RwSignal::new(false);
    let checking = RwSignal::new(false);
    let selected = RwSignal::new(false);
    let manual_token = RwSignal::new(String::new());
    let running = RwSignal::new(false);
    let connected = RwSignal::new(false);
    let bot_log = RwSignal::new(String::new());
    let log_seq = RwSignal::new(0u64);
    let live = RwSignal::new(false);
    let msg = RwSignal::new(String::new());
    let badge = RwSignal::new(BadgeStatus::default());
    let badge_log = RwSignal::new(String::new());
    let badge_seq = RwSignal::new(0u64);
    let badge_msg = RwSignal::new(String::new());
    let quit_prompt = RwSignal::new(None::<(f64, usize, bool)>);
    let quitting = RwSignal::new(false);
    let update = RwSignal::new(None::<UpdateInfo>);
    let update_progress = RwSignal::new(None::<UpdateProgress>);
    let update_error = RwSignal::new(String::new());
    let copied = RwSignal::new(false);
    let log_box = NodeRef::<leptos::html::Pre>::new();

    // Boot: load config, attach to the native engine, verify the saved token.
    spawn_local({
        let cfg = cfg;
        let profile = profile;
        let selected = selected;
        let msg = msg;
        let tokens = tokens;
        let notes = notes;
        let detecting = detecting;
        let running = running;
        let connected = connected;
        let live = live;
        let badge = badge;
        async move {
            let loaded = api::app_config().await;
            cfg.set(loaded.clone());

            let status = api::bot_status().await;
            running.set(status.running);
            connected.set(status.connected);
            if let Some(user) = status.user {
                profile.set(user);
            }
            live.set(true);

            let initial_badge = api::badge_status().await;
            badge.set(initial_badge);

            let token = loaded.value("DISCORD_TOKEN");
            if token.is_empty() {
                detecting.set(true);
                match api::scan_tokens().await {
                    Ok(report) => {
                        tokens.set(report.accounts.clone());
                        notes.set(report.notes);
                        if report.accounts.len() == 1 {
                            let account = report.accounts[0].clone();
                            profile.set(account.profile.clone());
                            selected.set(true);
                            set_value(cfg, "DISCORD_TOKEN", account.token.clone());
                            save_config(cfg);
                        }
                    }
                    Err(error) => notes.set(vec![error]),
                }
                detecting.set(false);
            } else {
                selected.set(true);
                match api::account_profile(token).await {
                    Ok(me) => profile.set(me),
                    Err(error) => msg.set(format!("Saved token could not be verified: {error}")),
                }
            }

            TimeoutFuture::new(5_000).await;
            if let Ok(info) = api::check_update().await {
                if info.available {
                    update.set(Some(info));
                }
            }
            loop {
                TimeoutFuture::new(30 * 60 * 1000).await;
                if let Ok(info) = api::check_update().await {
                    if info.available {
                        update.set(Some(info));
                    }
                }
            }
        }
    });

    // Poll the engine, badge run and updater state. No JS event listeners needed.
    spawn_local({
        let running = running;
        let connected = connected;
        let profile = profile;
        let live = live;
        let bot_log = bot_log;
        let log_seq = log_seq;
        let badge = badge;
        let badge_log = badge_log;
        let badge_seq = badge_seq;
        let update_progress = update_progress;
        async move {
            loop {
                TimeoutFuture::new(600).await;
                let status = api::bot_status().await;
                running.set(status.running);
                connected.set(status.connected);
                if let Some(user) = status.user {
                    profile.set(user);
                }
                live.set(true);

                let (chunk, next) = api::bot_logs(log_seq.get_untracked()).await;
                if !chunk.is_empty() {
                    append_log(bot_log, &chunk);
                    log_seq.set(next);
                }

                let badge_state = api::badge_status().await;
                badge.set(badge_state);

                let (chunk, next) = api::badge_logs(badge_seq.get_untracked()).await;
                if !chunk.is_empty() {
                    append_log(badge_log, &chunk);
                    badge_seq.set(next);
                }

                if let Some(progress) = api::update_progress().await {
                    update_progress.set(Some(progress));
                }
            }
        }
    });

    // Auto-scroll the log panes.
    Effect::new(move |_| {
        let _ = bot_log.get();
        if let Some(box_element) = log_box.get() {
            box_element.set_scroll_top(box_element.scroll_height());
        }
    });

    let scan = move || {
        let tokens = tokens;
        let notes = notes;
        let detecting = detecting;
        let cfg = cfg;
        let profile = profile;
        let selected = selected;
        detecting.set(true);
        spawn_local(async move {
            match api::scan_tokens().await {
                Ok(report) => {
                    tokens.set(report.accounts.clone());
                    notes.set(report.notes);
                    if report.accounts.len() == 1 {
                        choose_account(report.accounts[0].clone(), cfg, profile, selected);
                    }
                }
                Err(error) => {
                    tokens.set(Vec::new());
                    notes.set(vec![error]);
                }
            }
            detecting.set(false);
        });
    };

    let use_manual = move || {
        let token = manual_token.get_untracked().trim().to_string();
        if token.is_empty() {
            return;
        }
        let checking = checking;
        let msg = msg;
        let profile = profile;
        let selected = selected;
        let manual_token = manual_token;
        let cfg = cfg;
        checking.set(true);
        spawn_local(async move {
            match api::account_profile(token.clone()).await {
                Ok(me) => {
                    profile.set(me);
                    selected.set(true);
                    manual_token.set(String::new());
                    msg.set(String::new());
                    set_value(cfg, "DISCORD_TOKEN", token);
                    save_config(cfg);
                }
                Err(error) => msg.set(error),
            }
            checking.set(false);
        });
    };

    let switch_account = move || {
        selected.set(false);
        profile.set(Profile::default());
        manual_token.set(String::new());
        msg.set(String::new());
        tokens.set(Vec::new());
        notes.set(Vec::new());
        tab.set(Tab::Dashboard);
        scan();
    };

    let persist_and_start = move || {
        let cfg = cfg;
        let msg = msg;
        let bot_log = bot_log;
        let log_seq = log_seq;
        let running = running;
        let connected = connected;
        save_config(cfg);
        bot_log.set(String::new());
        log_seq.set(0);
        spawn_local(async move {
            match api::start_bot().await {
                Ok(()) => {
                    running.set(true);
                    connected.set(false);
                    msg.set("Bot started".into());
                }
                Err(error) => msg.set(error),
            }
        });
    };

    let restart = move || {
        let cfg = cfg;
        let msg = msg;
        let bot_log = bot_log;
        let log_seq = log_seq;
        let running = running;
        let connected = connected;
        save_config(cfg);
        bot_log.set(String::new());
        log_seq.set(0);
        spawn_local(async move {
            match api::restart_bot().await {
                Ok(()) => {
                    running.set(true);
                    connected.set(false);
                    msg.set("Bot restarted".into());
                }
                Err(error) => msg.set(error),
            }
        });
    };

    let stop = move || {
        let msg = msg;
        let running = running;
        let connected = connected;
        spawn_local(async move {
            match api::stop_bot().await {
                Ok(()) => {
                    running.set(false);
                    connected.set(false);
                    msg.set("Bot stopped".into());
                }
                Err(error) => msg.set(error),
            }
        });
    };

    let copy_token = move |_| {
        let token = cfg.get_untracked().value("DISCORD_TOKEN");
        if token.is_empty() {
            return;
        }
        let copied = copied;
        spawn_local(async move {
            if api::copy_text(token).await.is_ok() {
                copied.set(true);
                TimeoutFuture::new(2_000).await;
                copied.set(false);
            }
        });
    };

    let download = move |_| {
        let update_progress = update_progress;
        let update_error = update_error;
        update_error.set(String::new());
        update_progress.set(Some(UpdateProgress {
            phase: "downloading".into(),
            received: 0,
            total: None,
            percent: 0,
        }));
        spawn_local(async move {
            if let Err(error) = api::download_update().await {
                update_progress.set(None);
                update_error.set(error);
            }
        });
    };

    view! {
        <div class="app-shell">
            <TitleBar />
            <div class="app-body">
                <Rail
                    tab=tab
                    selected=selected
                    profile=profile
                    running=running
                    on_switch=switch_account
                />
                <section class="workspace">
                    <div class="workspace-scroll">
                        <div class="workspace-inner">
                            <Show when=move || update.get().is_some() fallback=|| ()>
                                <UpdateBanner
                                    update=update
                                    progress=update_progress
                                    error=update_error
                                    on_download=download
                                />
                            </Show>
                            <Show
                                when=move || selected.get()
                                fallback=move || view! {
                                    <LoginPanel
                                        tokens=tokens
                                        notes=notes
                                        detecting=detecting
                                        checking=checking
                                        manual_token=manual_token
                                        message=msg
                                        on_scan=scan
                                        on_manual=use_manual
                                        on_select=move |account| choose_account(account, cfg, profile, selected)
                                    />
                                }
                            >
                                {move || match tab.get() {
                                    Tab::Badges => view! {
                                        <BadgesPage
                                            cfg=cfg
                                            badge=badge
                                            log=badge_log
                                            message=badge_msg
                                            quit_prompt=quit_prompt
                                            quitting=quitting
                                        />
                                    }.into_any(),
                                    Tab::HypeSquad => view! {
                                        <HypeSquadPage token=cfg.get_untracked().value("DISCORD_TOKEN") />
                                    }.into_any(),
                                    Tab::Logs => view! {
                                        <LogsPage
                                            log=bot_log
                                            running=running
                                            live=live
                                            log_box=log_box
                                            on_clear=move |_| {
                                                bot_log.set(String::new());
                                                log_seq.set(0);
                                                spawn_local(async move { api::clear_bot_logs().await; });
                                            }
                                        />
                                    }.into_any(),
                                    Tab::Dashboard => view! {
                                        <Dashboard
                                            profile=profile
                                            cfg=cfg
                                            running=running
                                            connected=connected
                                            live=live
                                            message=msg
                                            copied=copied
                                            on_copy=copy_token
                                            on_start=move |_| persist_and_start()
                                            on_stop=move |_| stop()
                                            on_restart=move |_| restart()
                                            on_open_logs=move |_| tab.set(Tab::Logs)
                                        />
                                    }.into_any(),
                                }}
                            </Show>
                        </div>
                    </div>
                </section>
            </div>
        </div>
    }
}

// ------------------------------------------------------------------ title bar

#[component]
fn TitleBar() -> impl IntoView {
    view! {
        <header class="titlebar">
            <div class="titlebar-drag" data-tauri-drag-region="">
                <img class="titlebar-logo" src="./logo.png" alt="" />
                <div class="titlebar-title">
                    <span>"Discord Toolbox"</span>
                    <span class="titlebar-version">{env!("CARGO_PKG_VERSION")}</span>
                </div>
            </div>
            <div class="titlebar-controls">
                <button class="titlebar-button" title="Minimize" on:click=move |_| {
                    spawn_local(async { api::window_minimize().await });
                }>{icon::minimize()}</button>
                <button class="titlebar-button" title="Maximize" on:click=move |_| {
                    spawn_local(async { api::window_toggle_maximize().await });
                }>{icon::maximize()}</button>
                <button class="titlebar-button titlebar-button-close" title="Close" on:click=move |_| {
                    spawn_local(async { api::window_close().await });
                }>{icon::close()}</button>
            </div>
        </header>
    }
}

// ------------------------------------------------------------------ rail

#[component]
fn Rail(
    tab: RwSignal<Tab>,
    selected: RwSignal<bool>,
    profile: RwSignal<Profile>,
    running: RwSignal<bool>,
    on_switch: impl Fn() + 'static + Copy + Send + Sync,
) -> impl IntoView {
    let items = [
        (Tab::Dashboard, "Dashboard", "dashboard"),
        (Tab::Badges, "Game badges", "badges"),
        (Tab::HypeSquad, "HypeSquad", "hypesquad"),
        (Tab::Logs, "Logs", "logs"),
    ];

    view! {
        <aside class="rail">
            <div class="rail-brand">
                <img src="./logo.png" alt="" />
                <div>
                    <strong>"Toolbox"</strong>
                    <span>"Discord companion"</span>
                </div>
            </div>
            <nav class="rail-nav">
                {items
                    .into_iter()
                    .map(|(target, label, icon_name)| {
                        let icon_view = match icon_name {
                            "badges" => icon::badges().into_any(),
                            "hypesquad" => icon::hypesquad().into_any(),
                            "logs" => icon::logs().into_any(),
                            _ => icon::dashboard().into_any(),
                        };
                        view! {
                            <button
                                class="rail-button"
                                class:rail-button-active=move || tab.get() == target
                                disabled=move || !selected.get()
                                on:click=move |_| tab.set(target)
                            >
                                {icon_view}
                                <span>{label}</span>
                            </button>
                        }
                    })
                    .collect_view()}
            </nav>
            <div class="rail-footer">
                <div class="rail-account">
                    <Avatar profile=profile size=34 />
                    <div class="rail-account-text">
                        <strong>{move || profile.get().display_name()}</strong>
                        <span>
                            <i class="status-dot" class:online=move || running.get()></i>
                            {move || if running.get() { "Online" } else { "Offline" }}
                        </span>
                    </div>
                </div>
                <button class="rail-switch" on:click=move |_| on_switch()>
                    "Switch account"
                </button>
            </div>
        </aside>
    }
}

// ------------------------------------------------------------------ small pieces

#[component]
fn Avatar(profile: RwSignal<Profile>, #[prop(default = 44)] size: u32) -> impl IntoView {
    let initials = Memo::new(move |_| profile.get().initials());
    view! {
        <div class="avatar" style=format!("width:{size}px;height:{size}px")>
            <Show when=move || profile.get().avatar_url.is_some() fallback=move || view! { <span>{move || initials.get()}</span> }>
                <img src=move || profile.get().avatar_url.unwrap_or_default() alt="" />
            </Show>
        </div>
    }
}

#[component]
fn PageHeader(
    title: &'static str,
    subtitle: &'static str,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    view! {
        <div class="page-header">
            <div>
                <h1>{title}</h1>
                <p>{subtitle}</p>
            </div>
            {children.map(|children| children())}
        </div>
    }
}

#[component]
fn Field(
    label: &'static str,
    hint: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <label class="field">
            <span class="field-label">{label}</span>
            <span class="field-hint">{hint}</span>
            {children()}
        </label>
    }
}

#[component]
fn LoginPanel(
    tokens: RwSignal<Vec<Account>>,
    notes: RwSignal<Vec<String>>,
    detecting: RwSignal<bool>,
    checking: RwSignal<bool>,
    manual_token: RwSignal<String>,
    message: RwSignal<String>,
    on_scan: impl Fn() + 'static + Copy + Send + Sync,
    on_manual: impl Fn() + 'static + Copy + Send + Sync,
    on_select: impl Fn(Account) + 'static + Copy + Send + Sync,
) -> impl IntoView {
    view! {
        <div class="fade-in login">
            <div class="login-orb"></div>
            <PageHeader
                title="Connect your account"
                subtitle="Discord Toolbox keeps your account online without touching your password. Tokens stay on this computer."
            />
            <div class="grid two">
                <div class="card login-card">
                    <div class="card-heading">
                        <div>
                            <h2>"Detected accounts"</h2>
                            <p>"Read from local Discord clients on this PC."</p>
                        </div>
                        <button class="button secondary small" disabled=move || detecting.get() on:click=move |_| on_scan()>
                            {icon::refresh()}
                            {move || if detecting.get() { "Scanning…" } else { "Scan again" }}
                        </button>
                    </div>
                    <Show when=move || !tokens.get().is_empty() fallback=move || view! {
                        <div class="empty">
                            <p>"No local Discord tokens were found."</p>
                            <span>"Sign in to the Discord desktop app, or paste a token below."</span>
                        </div>
                    }>
                        <div class="account-list">
                            <For each=move || tokens.get() key=|account| account.token.clone() children=move |account| {
                                let picked = account.clone();
                                view! {
                                    <button class="account-row" on:click=move |_| on_select(picked.clone())>
                                        <Avatar profile=RwSignal::new(account.profile.clone()) size=38 />
                                        <div class="account-row-text">
                                            <strong>{account.profile.display_name()}</strong>
                                            <span>{account.profile.handle()}</span>
                                        </div>
                                        <span class="pill">{account.client.clone()}</span>
                                    </button>
                                }
                            } />
                        </div>
                    </Show>
                    <Show when=move || !notes.get().is_empty() fallback=|| ()>
                        <ul class="notes">
                            <For each=move || notes.get() key=|note| note.clone() children=move |note| view! { <li>{note}</li> } />
                        </ul>
                    </Show>
                </div>

                <div class="card login-card">
                    <div class="card-heading">
                        <div>
                            <h2>"Use a token"</h2>
                            <p>"Paste a Discord user token to sign in manually."</p>
                        </div>
                    </div>
                    <Field label="Discord token" hint="Stored in .env next to the app.">
                        <input
                            class="input"
                            type="password"
                            placeholder="Paste your token"
                            autocomplete="off"
                            prop:value=move || manual_token.get()
                            on:input=move |event| manual_token.set(event_value(&event))
                            on:keydown=move |event| {
                                if event.key() == "Enter" {
                                    on_manual();
                                }
                            }
                        />
                    </Field>
                    <button class="button primary wide" disabled=move || checking.get() || manual_token.get().trim().is_empty() on:click=move |_| on_manual()>
                        {icon::user()}
                        {move || if checking.get() { "Checking token…" } else { "Use this token" }}
                    </button>
                    <Show when=move || !message.get().is_empty() fallback=|| ()>
                        <p class="message error">{move || message.get()}</p>
                    </Show>
                    <div class="divider"></div>
                    <p class="fine-print">
                        "Automating a user account is against Discord's Terms of Service. Use this at your own risk."
                    </p>
                </div>
            </div>
        </div>
    }
}

// ------------------------------------------------------------------ dashboard

#[component]
fn Dashboard(
    profile: RwSignal<Profile>,
    cfg: RwSignal<Config>,
    running: RwSignal<bool>,
    connected: RwSignal<bool>,
    live: RwSignal<bool>,
    message: RwSignal<String>,
    copied: RwSignal<bool>,
    on_copy: impl Fn(MouseEvent) + 'static + Copy + Send + Sync,
    on_start: impl Fn(MouseEvent) + 'static + Copy + Send + Sync,
    on_stop: impl Fn(MouseEvent) + 'static + Copy + Send + Sync,
    on_restart: impl Fn(MouseEvent) + 'static + Copy + Send + Sync,
    on_open_logs: impl Fn(MouseEvent) + 'static + Copy + Send + Sync,
) -> impl IntoView {
    view! {
        <div class="fade-in">
            <PageHeader title="Dashboard" subtitle="Your account, live from the native Rust engine.">
                <span class="live-badge" class:live=move || live.get() && connected.get() class:warn=move || live.get() && running.get() && !connected.get()>
                    <i></i>
                    {move || if !running.get() { "Stopped" } else if connected.get() { "Running" } else { "Connecting" }}
                </span>
            </PageHeader>

            <div class="card token-card">
                <div class="token-icon">{icon::alert()}</div>
                <div class="token-copy">
                    <h2>"Your Discord token"</h2>
                    <p>
                        "A token grants full access to your account. Only copy it for your own use, never share it, and never paste it into another site or app."
                    </p>
                    <div class="token-value">
                        <code>{move || {
                            let token = cfg.get().value("DISCORD_TOKEN");
                            if token.is_empty() {
                                "No token saved".to_string()
                            } else if token.len() > 18 {
                                format!("{}…{}", &token[..10], &token[token.len() - 6..])
                            } else {
                                token
                            }
                        }}</code>
                        <button
                            class="button secondary small"
                            disabled=move || cfg.get().value("DISCORD_TOKEN").is_empty()
                            on:click=move |event| on_copy(event)
                        >
                            {move || if copied.get() { icon::check().into_any() } else { icon::copy().into_any() }}
                            {move || if copied.get() { "Copied" } else { "Copy token" }}
                        </button>
                    </div>
                </div>
            </div>

            <div class="grid dashboard-grid">
                <div class="card account-card">
                    <div class="account-main">
                        <Avatar profile=profile size=64 />
                        <div class="account-details">
                            <h2>{move || profile.get().display_name()}</h2>
                            <p>{move || profile.get().handle()}</p>
                            <div class="account-meta">
                                <span class="pill">{move || profile.get().id.clone().unwrap_or_default()}</span>
                            </div>
                        </div>
                    </div>
                </div>

                <div class="card control-card">
                    <h2>"Engine"</h2>
                    <p class="muted">"The Rust engine keeps your presence alive while the app is open."</p>
                    <div class="status-line">
                        <i class="status-dot" class:online=move || connected.get() class:pending=move || running.get() && !connected.get()></i>
                        <div>
                            <strong>{move || if !running.get() { "Stopped" } else if connected.get() { "Connected" } else { "Connecting" }}</strong>
                            <span>{move || if connected.get() { "Presence is being maintained" } else if running.get() { "Retrying the Discord gateway" } else { "Press Start to connect" }}</span>
                        </div>
                    </div>
                    <div class="button-row">
                        <button
                            class="button primary"
                            disabled=move || running.get()
                            on:click=move |event| on_start(event)
                        >
                            {icon::play()}"Start"
                        </button>
                        <button
                            class="button danger"
                            disabled=move || !running.get()
                            on:click=move |event| on_stop(event)
                        >
                            {icon::stop()}"Stop"
                        </button>
                        <button class="button secondary" on:click=move |event| on_restart(event)>
                            {icon::restart()}"Restart"
                        </button>
                    </div>
                    <button class="link-button" on:click=move |event| on_open_logs(event)>
                        "Open live logs" {icon::external()}
                    </button>
                    <Show when=move || !message.get().is_empty() fallback=|| ()>
                        <p class="message">{move || message.get()}</p>
                    </Show>
                </div>
            </div>
        </div>
    }
}

// ------------------------------------------------------------------ badges

#[component]
fn BadgesPage(
    cfg: RwSignal<Config>,
    badge: RwSignal<BadgeStatus>,
    log: RwSignal<String>,
    message: RwSignal<String>,
    quit_prompt: RwSignal<Option<(f64, usize, bool)>>,
    quitting: RwSignal<bool>,
) -> impl IntoView {
    let hours = RwSignal::new("4.9935920412842105e301".to_string());
    let games = RwSignal::new(String::new());
    let repeat = RwSignal::new(false);

    let forge = move || {
        let token = cfg.get_untracked().value("DISCORD_TOKEN");
        let hours_value = hours.get_untracked().parse::<f64>().unwrap_or(0.0);
        let games_raw = games.get_untracked().trim().to_string();
        let games_value = if games_raw.is_empty() {
            0
        } else {
            match games_raw.parse::<usize>() {
                Ok(value) if value >= 1 => value,
                _ => {
                    message.set("Games must be blank (all games) or at least 1.".into());
                    return;
                }
            }
        };
        let repeat_value = repeat.get_untracked();
        let message = message;
        let quit_prompt = quit_prompt;
        message.set(String::new());
        spawn_local(async move {
            match api::forge_badges(token, hours_value, games_value, repeat_value).await {
                Ok(()) => {}
                Err(error) if error.starts_with("COOKIE_LOCKED:") => {
                    quit_prompt.set(Some((hours_value, games_value, repeat_value)));
                }
                Err(error) => message.set(error.trim_start_matches("COOKIE_LOCKED:").to_string()),
            }
        });
    };

    let confirm_quit = move |_| {
        let Some((hours_value, games_value, repeat_value)) = quit_prompt.get_untracked() else {
            return;
        };
        let quitting = quitting;
        let message = message;
        let cfg = cfg;
        quitting.set(true);
        spawn_local(async move {
            match api::quit_discord().await {
                Ok(_) => {
                    TimeoutFuture::new(1_500).await;
                    quit_prompt.set(None);
                    let token = cfg.get_untracked().value("DISCORD_TOKEN");
                    if let Err(error) = api::forge_badges(token, hours_value, games_value, repeat_value).await {
                        message.set(error.trim_start_matches("COOKIE_LOCKED:").to_string());
                    }
                }
                Err(error) => {
                    quit_prompt.set(None);
                    message.set(error);
                }
            }
            quitting.set(false);
        });
    };

    view! {
        <div class="fade-in">
            <PageHeader title="Game badges" subtitle="Credit playtime through Discord's own game telemetry.">
                <span class="live-badge" class:live=move || badge.get().running>
                    <i></i>{move || {
                        let status = badge.get();
                        if status.running {
                            if status.repeat { "Looping" } else { "Forging" }
                        } else { "Idle" }
                    }}
                </span>
            </PageHeader>

            <div class="grid two">
                <div class="card">
                    <h2>"Run a forge"</h2>
                    <p class="muted">"One pass by default. Enable looping to keep going until Stop."</p>
                    <div class="form-grid">
                        <Field label="Total hours" hint="Maximum 4.9936e301 per run.">
                            <input class="input" type="number" min="0.1" max="4.9935920412842105e301" step="any" prop:value=move || hours.get() on:input=move |event| hours.set(event_value(&event)) />
                        </Field>
                        <Field label="Games" hint="Leave blank for all games. Minimum 1.">
                            <input class="input" type="number" min="1" step="1" placeholder="All games" prop:value=move || games.get() on:input=move |event| games.set(event_value(&event)) />
                        </Field>
                    </div>
                    <label class="loop-toggle">
                        <input
                            type="checkbox"
                            prop:checked=move || repeat.get()
                            disabled=move || badge.get().running
                            on:change=move |event| repeat.set(event_checked(&event))
                        />
                        <span class="toggle-track" aria-hidden="true"><span class="toggle-thumb"></span></span>
                        <span>"Loop until stopped"</span>
                    </label>
                    <div class="button-row">
                        <button class="button primary" disabled=move || badge.get().running on:click=move |_| forge()>
                            {icon::spark()}{move || if repeat.get() { "Start looping" } else { "Start forging" }}
                        </button>
                        <button class="button secondary" disabled=move || !badge.get().running on:click=move |_| {
                            spawn_local(async { api::stop_badges().await });
                        }>{icon::stop()}"Stop"</button>
                    </div>
                    <Show when=move || badge.get().running fallback=|| ()>
                        <div class="forge-progress">
                            <div class="progress-track"><div class="progress-fill" style=move || {
                                let status = badge.get();
                                let total = status.games.unwrap_or(0);
                                let percent = if total > 0 { (status.processed.saturating_mul(100) / total).min(100) } else { 0 };
                                format!("width:{percent}%")
                            }></div></div>
                            <span>{move || {
                                let status = badge.get();
                                let total = status.games.unwrap_or(0);
                                let processed = status.processed.min(total);
                                let prefix = if status.repeat { format!("Pass {} · ", status.pass.max(1)) } else { String::new() };
                                if total == 0 {
                                    format!("{prefix}Preparing…")
                                } else {
                                    format!("{prefix}{} of {} games credited", format_count(processed), format_count(total))
                                }
                            }}</span>
                        </div>
                    </Show>
                    <Show when=move || badge.get().error.is_some() fallback=|| ()>
                        <p class="message error">{move || badge.get().error.clone().unwrap_or_default()}</p>
                    </Show>
                    <Show when=move || !message.get().is_empty() fallback=|| ()>
                        <p class="message error">{move || message.get()}</p>
                    </Show>
                    <Show when=move || badge.get().finished && badge.get().username.is_some() fallback=|| ()>
                        <p class="message success">{move || format!(
                            "Finished for @{} · {} games credited",
                            badge.get().username.clone().unwrap_or_default(),
                            format_count(badge.get().processed)
                        )}</p>
                    </Show>
                    <Show when=move || badge.get().stopped && badge.get().username.is_some() fallback=|| ()>
                        <p class="message">{move || format!(
                            "Stopped for @{} · {} games credited",
                            badge.get().username.clone().unwrap_or_default(),
                            format_count(badge.get().processed)
                        )}</p>
                    </Show>
                </div>
                <div class="card">
                    <div class="card-heading">
                        <h2>"Forge log"</h2>
                        <button class="button ghost small" on:click=move |_| log.set(String::new())>{icon::trash()}"Clear"</button>
                    </div>
                    <pre class="log-box badge-log">{move || {
                        let value = log.get();
                        if value.is_empty() { "The forge log will appear here.".to_string() } else { value }
                    }}</pre>
                </div>
            </div>

            <Show when=move || quit_prompt.get().is_some() fallback=|| ()>
                <div class="modal-backdrop">
                    <div class="modal">
                        <div class="modal-icon">{icon::alert()}</div>
                        <h2>"Quit Discord?"</h2>
                        <p>"Discord keeps its cookie store locked while it is open. The forger needs that cookie to sign the telemetry request. Discord will be closed and the forge will continue automatically."</p>
                        <div class="button-row end">
                            <button class="button ghost" on:click=move |_| quit_prompt.set(None)>"Cancel"</button>
                            <button class="button danger" disabled=move || quitting.get() on:click=confirm_quit>
                                {move || if quitting.get() { "Quitting…" } else { "Quit Discord & forge" }}
                            </button>
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}

// ------------------------------------------------------------------ hypesquad

#[component]
fn HypeSquadPage(token: String) -> impl IntoView {
    let status = RwSignal::new(None::<HypeSquadStatus>);
    let loading = RwSignal::new(true);
    let message = RwSignal::new(String::new());

    let refresh_token = token.clone();
    let refresh = move || {
        let token = refresh_token.clone();
        loading.set(true);
        spawn_local(async move {
            match api::hypesquad_status(token).await {
                Ok(value) => status.set(Some(value)),
                Err(error) => message.set(error),
            }
            loading.set(false);
        });
    };
    refresh();

    let view_token = StoredValue::new(token.clone());
    view! {
        <div class="fade-in">
            <PageHeader title="HypeSquad" subtitle="Switch your house, attempt removal, and inspect profile badges.">
                <button class="button secondary small" disabled=move || loading.get() on:click=move |_| refresh()>
                    {icon::refresh()}"Refresh"
                </button>
            </PageHeader>

            <Show when=move || loading.get() fallback=move || view! {
                <Show when=move || status.get().is_some() fallback=move || view! {
                    <div class="card"><p class="muted">{move || if message.get().is_empty() { "No HypeSquad data available.".to_string() } else { message.get() }}</p></div>
                }>
                    {move || {
                        let value = status.get().unwrap_or_default();
                        let house = value.house.clone().unwrap_or_else(|| "none".into());
                        let bravery_active = house == "bravery";
                        let brilliance_active = house == "brilliance";
                        let balance_active = house == "balance";
                        let bravery_token = view_token.get_value();
                        let brilliance_token = view_token.get_value();
                        let balance_token = view_token.get_value();
                        let remove_token = view_token.get_value();
                        view! {
                            <div class="grid two">
                                <div class="card hs-profile">
                                    <div class="hs-head">
                                        <div class="hs-avatar">
                                            {value.avatar_url.clone().map(|url| view! { <img src=url alt="" /> })}
                                        </div>
                                        <div>
                                            <h2>{value.display_name.clone()}</h2>
                                            <p class="muted">{format!("@{}", value.username)}</p>
                                        </div>
                                    </div>
                                    <div class="hs-house">
                                        <span class="eyebrow">"Current house"</span>
                                        <strong class=format!("house {house}")>{house.to_uppercase()}</strong>
                                    </div>
                                    <div class="hs-choices">
                                        <button
                                            class="hs-choice hs-bravery"
                                            class:selected=bravery_active
                                            title="Switch to Bravery"
                                            aria-pressed=if bravery_active { "true" } else { "false" }
                                            on:click=move |_| {
                                                let token = bravery_token.clone();
                                                message.set(String::new());
                                                spawn_local(async move {
                                                    match api::set_hypesquad(token, "bravery".into()).await {
                                                        Ok(value) => status.set(Some(value)),
                                                        Err(error) => message.set(error),
                                                    }
                                                });
                                            }
                                        >
                                            <img src="https://cdn.discordapp.com/badge-icons/8a88d63823d8a71cd5e390baa45efa02.png" alt="Bravery badge" />
                                            <span>"Bravery"</span>
                                        </button>
                                        <button
                                            class="hs-choice hs-brilliance"
                                            class:selected=brilliance_active
                                            title="Switch to Brilliance"
                                            aria-pressed=if brilliance_active { "true" } else { "false" }
                                            on:click=move |_| {
                                                let token = brilliance_token.clone();
                                                message.set(String::new());
                                                spawn_local(async move {
                                                    match api::set_hypesquad(token, "brilliance".into()).await {
                                                        Ok(value) => status.set(Some(value)),
                                                        Err(error) => message.set(error),
                                                    }
                                                });
                                            }
                                        >
                                            <img src="https://cdn.discordapp.com/badge-icons/011940fd013da3f7fb926e4a1cd2e618.png" alt="Brilliance badge" />
                                            <span>"Brilliance"</span>
                                        </button>
                                        <button
                                            class="hs-choice hs-balance"
                                            class:selected=balance_active
                                            title="Switch to Balance"
                                            aria-pressed=if balance_active { "true" } else { "false" }
                                            on:click=move |_| {
                                                let token = balance_token.clone();
                                                message.set(String::new());
                                                spawn_local(async move {
                                                    match api::set_hypesquad(token, "balance".into()).await {
                                                        Ok(value) => status.set(Some(value)),
                                                        Err(error) => message.set(error),
                                                    }
                                                });
                                            }
                                        >
                                            <img src="https://cdn.discordapp.com/badge-icons/3aa41de486fa12454c3761e8e223442e.png" alt="Balance badge" />
                                            <span>"Balance"</span>
                                        </button>
                                    </div>
                                    <button class="link-button danger-text" on:click=move |_| {
                                        let token = remove_token.clone();
                                        message.set(String::new());
                                        spawn_local(async move {
                                            match api::remove_hypesquad(token).await {
                                                Ok(value) => status.set(Some(value)),
                                                Err(error) => message.set(error),
                                            }
                                        });
                                    }>"Attempt to remove my HypeSquad badge"</button>
                                    <Show when=move || !message.get().is_empty() fallback=|| ()>
                                        <p class="message error">{move || message.get()}</p>
                                    </Show>
                                </div>
                                <div class="card">
                                    <h2>"Profile badges"</h2>
                                    <Show when=move || !status.get().unwrap_or_default().badges.is_empty() fallback=move || view! {
                                        <p class="muted">"No profile badges were returned."</p>
                                    }>
                                        <div class="badge-list">
                                            <For each=move || status.get().unwrap_or_default().badges key=|badge| badge.id.clone() children=move |badge| {
                                                view! {
                                                    <div class="badge-row">
                                                        {badge.icon_url.map(|url| view! { <img src=url alt="" /> })}
                                                        <span>{badge.description}</span>
                                                    </div>
                                                }
                                            } />
                                        </div>
                                    </Show>
                                </div>
                            </div>
                        }
                    }}
                </Show>
            }>
                <div class="card"><p class="muted">"Loading HypeSquad status…"</p></div>
            </Show>
        </div>
    }
}

// ------------------------------------------------------------------ logs

#[component]
fn LogsPage(
    log: RwSignal<String>,
    running: RwSignal<bool>,
    live: RwSignal<bool>,
    log_box: NodeRef<leptos::html::Pre>,
    on_clear: impl Fn(MouseEvent) + 'static + Copy + Send + Sync,
) -> impl IntoView {
    view! {
        <div class="fade-in logs-page">
            <PageHeader title="Logs" subtitle="Live output from the Rust gateway engine.">
                <div class="button-row">
                    <span class="live-badge" class:live=move || live.get() && running.get()>
                        <i></i>{move || if running.get() { "Live" } else { "Idle" }}
                    </span>
                    <button class="button ghost small" on:click=move |event| on_clear(event)>{icon::trash()}"Clear"</button>
                </div>
            </PageHeader>
            <pre class="log-box main-log" node_ref=log_box>{move || {
                let value = log.get();
                if value.is_empty() { "No log lines yet — press Start on the Dashboard.".to_string() } else { value }
            }}</pre>
        </div>
    }
}

// ------------------------------------------------------------------ update

#[component]
fn UpdateBanner(
    update: RwSignal<Option<UpdateInfo>>,
    progress: RwSignal<Option<UpdateProgress>>,
    error: RwSignal<String>,
    on_download: impl Fn(MouseEvent) + 'static + Copy + Send + Sync,
) -> impl IntoView {
    view! {
        <div class="update-banner">
            <div class="update-icon">{icon::spark()}</div>
            <div class="update-text">
                <strong>{move || {
                    let info = update.get();
                    match info.and_then(|value| value.version) {
                        Some(version) => format!("Discord Toolbox {version} is available"),
                        None => "An update is available".into(),
                    }
                }}</strong>
                <span>{move || {
                    progress.get().map(|value| {
                        if value.phase == "installing" {
                            "Installing — the app will restart.".to_string()
                        } else {
                            format!("Downloading… {}%", value.percent)
                        }
                    }).or_else(|| update.get().and_then(|info| info.asset_name)).unwrap_or_else(|| "A newer build is on GitHub.".into())
                }}</span>
                <Show when=move || progress.get().is_some() fallback=|| ()>
                    <div class="progress-track">
                        <div class="progress-fill" style=move || format!("width:{}%", progress.get().map(|value| value.percent).unwrap_or(0))></div>
                    </div>
                </Show>
                <Show when=move || !error.get().is_empty() fallback=|| ()>
                    <span class="message error">{move || error.get()}</span>
                </Show>
            </div>
            <div class="update-actions">
                <button class="button secondary small" disabled=move || progress.get().is_some() on:click=move |event| on_download(event)>
                    "Update and restart"
                </button>
                <button class="button ghost small" disabled=move || progress.get().is_some() on:click=move |_| update.set(None)>"Later"</button>
            </div>
        </div>
    }
}
