//! The 账户 page: saved accounts, Microsoft sign-in and the Minecraft profile (name, skin, cape).

use super::controller::{Controller, Msg, put};
use super::model;
use crate::game::account::{Account, AccountStore};
use crate::net::auth::{self, AuthError, ensure_fresh};
use crate::net::profile::{self, McProfile};
use std::fs;
use std::io::Read;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

/// Opens `url` in the default browser; failures are ignored since the dialog also shows the URL.
fn open_in_browser(url: &str) {
    #[cfg(target_os = "windows")]
    let child = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let child = Command::new("open").arg(url).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let child = Command::new("xdg-open").arg(url).spawn();
    if let Ok(mut child) = child {
        thread::spawn(move || child.wait());
    }
}

impl Controller {
    fn accounts(&self) -> AccountStore {
        self.model.borrow().accounts.clone()
    }

    fn save_accounts(&self) {
        if let Err(e) = self.model.borrow().accounts.save(&AccountStore::default_path()) {
            put(&self.shell.account_status, format!("保存账户失败：{e}"));
        }
    }

    fn edit_accounts(&self, edit: impl FnOnce(&mut AccountStore)) {
        edit(&mut self.model.borrow_mut().accounts);
        self.save_accounts();
        self.reload_accounts();
    }

    /// Rebuilds the account list, the play dropdown and the selected account's details.
    pub(super) fn reload_accounts(&self) {
        let sh = &self.shell;
        let store = self.accounts();
        put(&sh.accounts, model::account_records(&store.accounts));
        let mut titles: Vec<String> = store.accounts.iter().map(|a| a.username().to_string()).collect();
        if titles.is_empty() {
            titles.push("无账户".to_string());
        }
        put(&sh.account_titles, titles);
        put(&sh.account_index, store.selected.min(store.accounts.len().saturating_sub(1)) as i64);

        let Some(account) = store.selected() else {
            put(&sh.account, String::new());
            put(&sh.account_microsoft, false);
            return;
        };
        let changed = sh.account.get() != account.uuid();
        put(&sh.account, account.uuid().to_string());
        put(&sh.account_title, account.username().to_string());
        let kind = if account.is_microsoft() { "微软账户 · 正版验证" } else { "离线账户" };
        put(&sh.account_detail, format!("{kind} · UUID {}", account.uuid()));
        put(&sh.account_microsoft, account.is_microsoft());
        if changed {
            put(&sh.account_status, String::new());
            sh.new_name.set_text(account.username());
            self.show_profile(None);
        }
        // Load the profile once per account (again after a new sign-in clears `profile_of`).
        if account.is_microsoft() && self.model.borrow().profile_of != account.uuid() && !sh.account_busy.get() {
            self.profile_op("正在读取档案…", profile::fetch_profile);
        }
    }

    /// Fills the skin and cape controls; `None` clears them while the profile loads.
    fn show_profile(&self, profile: Option<&McProfile>) {
        let sh = &self.shell;
        let skin = profile.and_then(McProfile::active_skin);
        put(&sh.skin_slim, skin.is_some_and(|s| s.is_slim()));
        put(&sh.skin_text, match (profile, skin) {
            (None, _) => String::new(),
            (Some(_), Some(s)) => format!("当前皮肤使用{}模型。上传 64×64 的 PNG 图片来更换。", if s.is_slim() { "纤细" } else { "经典" }),
            (Some(_), None) => "当前使用默认皮肤。上传 64×64 的 PNG 图片来更换。".to_string(),
        });
        put(&sh.skin_shown, false);
        if let Some(skin) = skin.filter(|_| self.online) {
            let (uuid, url, post) = (profile.map_or(String::new(), McProfile::hyphenated_id), skin.url.clone(), self.post.clone());
            thread::spawn(move || {
                let result = crate::net::client::http_get(&url).map_err(|e| e.to_string()).and_then(|resp| {
                    let mut png = Vec::new();
                    resp.into_reader().take(1 << 20).read_to_end(&mut png).map_err(|e| e.to_string())?;
                    Ok(png)
                });
                post(Msg::SkinTexture { uuid, result });
            });
        }
        let capes = profile.map_or(&[][..], |p| &p.capes[..]);
        put(&sh.capes, model::cape_records(capes));
        put(&sh.cape_none, profile.is_none_or(|p| p.active_cape().is_none()));
    }

    /// Selects the account used to launch.
    pub(super) fn pick_account(&self, uuid: &str) {
        let Some(index) = self.model.borrow().accounts.position(uuid) else { return };
        self.pick_account_index(index);
    }

    pub(super) fn pick_account_index(&self, index: usize) {
        if index >= self.model.borrow().accounts.accounts.len() || index == self.model.borrow().accounts.selected {
            return;
        }
        self.edit_accounts(|s| s.selected = index);
    }

    pub(super) fn add_offline(&self, name: &str) {
        let name = name.trim();
        if !Account::validate_username(name) {
            put(&self.shell.account_status, "离线玩家名需为 1–16 位字母、数字或下划线".into());
            self.shell.offline_name.focus();
            return;
        }
        self.shell.offline_name.set_text("");
        self.edit_accounts(|s| {
            s.upsert(Account::new_offline(name));
        });
    }

    pub(super) fn remove_account(&self, uuid: &str) {
        self.edit_accounts(|s| {
            s.remove(uuid);
        });
    }

    // ---------------- 微软登录 ----------------

    pub(super) fn start_login(&self) {
        let sh = &self.shell;
        let cancel = Arc::new(AtomicBool::new(false));
        let old = std::mem::replace(&mut self.model.borrow_mut().login_cancel, cancel.clone());
        old.store(true, Ordering::SeqCst);
        put(&sh.login_code, String::new());
        put(&sh.login_url, String::new());
        put(&sh.login_status, "正在向微软请求登录代码…".into());
        put(&sh.login_open, true);
        if !self.online {
            return;
        }
        let post = self.post.clone();
        thread::spawn(move || {
            let code = match auth::request_device_code() {
                Ok(code) => code,
                Err(e) => return post(Msg::DeviceCode(Err(e.to_string()))),
            };
            post(Msg::DeviceCode(Ok(code.clone())));
            let result = auth::poll_device_code(&code, &cancel).and_then(|ms| {
                if cancel.load(Ordering::SeqCst) {
                    return Err(AuthError::Cancelled);
                }
                auth::account_from_ms_token(&ms)
            });
            if !cancel.load(Ordering::SeqCst) {
                post(Msg::SignedIn(result.map_err(|e| e.to_string())));
            }
        });
    }

    pub(super) fn cancel_login(&self) {
        self.model.borrow().login_cancel.store(true, Ordering::SeqCst);
    }

    pub(super) fn open_login_page(&self) {
        let url = self.shell.login_url.get();
        if self.online && !url.is_empty() {
            open_in_browser(&url);
        }
    }

    pub(super) fn on_device_code(&self, result: Result<auth::DeviceCode, String>) {
        let sh = &self.shell;
        match result {
            Ok(code) => {
                put(&sh.login_status, "已在浏览器中打开微软登录页，请输入下面的代码；如果没有打开，请手动访问下面的网址。登录完成后这里会自动继续。".into());
                put(&sh.login_url, code.verification_uri.clone());
                put(&sh.login_code, code.user_code.clone());
                self.open_login_page();
            }
            Err(e) => put(&sh.login_status, format!("无法开始登录：{e}")),
        }
    }

    pub(super) fn on_signed_in(&self, result: Result<Account, String>) {
        let sh = &self.shell;
        match result {
            Ok(account) => {
                put(&sh.login_open, false);
                put(&sh.account_status, format!("已登录 {}", account.username()));
                self.model.borrow_mut().profile_of.clear();
                self.edit_accounts(|s| {
                    s.upsert(account);
                });
                self.goto(1);
            }
            Err(e) => {
                put(&sh.login_code, String::new());
                put(&sh.login_status, format!("登录失败：{e}"));
            }
        }
    }

    pub(super) fn on_account_updated(&self, result: Result<Account, String>) {
        match result {
            Ok(account) => self.edit_accounts(|s| s.update(account)),
            Err(e) => {
                put(&self.shell.account_busy, false);
                put(&self.shell.account_status, format!("验证失败：{e}"));
            }
        }
    }

    /// Signs the selected Microsoft account in again and reloads its profile.
    pub(super) fn refresh_account(&self) {
        let Some(account) = self.selected_microsoft() else { return };
        put(&self.shell.account_status, "正在重新验证…".into());
        put(&self.shell.account_busy, true);
        let post = self.post.clone();
        thread::spawn(move || match auth::refresh_account(&account) {
            Ok(fresh) => {
                let (uuid, token) = (fresh.uuid().to_string(), fresh.access_token().to_string());
                post(Msg::AccountUpdated(Ok(fresh)));
                post(Msg::Profile { uuid, result: profile::fetch_profile(&token).map_err(|e| e.to_string()) });
            }
            Err(e) => post(Msg::AccountUpdated(Err(e.to_string()))),
        });
    }

    // ---------------- 档案 ----------------

    fn selected_microsoft(&self) -> Option<Account> {
        let account = self.model.borrow().accounts.selected().cloned()?;
        (account.is_microsoft() && self.online).then_some(account)
    }

    /// Runs a profile request for the selected Microsoft account in the background, refreshing
    /// its token first when needed.
    fn profile_op(&self, status: &str, op: impl FnOnce(&str) -> Result<McProfile, AuthError> + Send + 'static) {
        let Some(account) = self.selected_microsoft() else { return };
        put(&self.shell.account_status, status.to_string());
        put(&self.shell.account_busy, true);
        let post = self.post.clone();
        thread::spawn(move || {
            let uuid = account.uuid().to_string();
            let token = match ensure_fresh(&account) {
                Ok(Some(fresh)) => {
                    let token = fresh.access_token().to_string();
                    post(Msg::AccountUpdated(Ok(fresh)));
                    token
                }
                Ok(None) => account.access_token().to_string(),
                Err(e) => return post(Msg::Profile { uuid, result: Err(e.to_string()) }),
            };
            post(Msg::Profile { uuid, result: op(&token).map_err(|e| e.to_string()) });
        });
    }

    pub(super) fn on_profile(&self, uuid: &str, result: Result<McProfile, String>) {
        let sh = &self.shell;
        put(&sh.account_busy, false);
        if sh.account.get() != uuid {
            return;
        }
        match result {
            Ok(profile) => {
                put(&sh.account_status, String::new());
                self.model.borrow_mut().profile_of = uuid.to_string();
                self.show_profile(Some(&profile));
                // The name may have changed here or on minecraft.net.
                let renamed = self.model.borrow().accounts.selected().filter(|a| a.username() != profile.name).cloned();
                if let Some(Account::Microsoft { uuid, access_token, refresh_token, expires_at, .. }) = renamed {
                    sh.new_name.set_text(&profile.name);
                    let account = Account::Microsoft { username: profile.name.clone(), uuid, access_token, refresh_token, expires_at };
                    self.edit_accounts(|s| s.update(account));
                }
            }
            Err(e) => put(&sh.account_status, e),
        }
    }

    /// Draws the front view of the downloaded skin texture.
    pub(super) fn on_skin_texture(&self, uuid: &str, result: Result<Vec<u8>, String>) {
        let sh = &self.shell;
        if sh.account.get() != uuid {
            return;
        }
        let figure = result.ok().and_then(|png| aegle::image::decode(&png).ok()).and_then(|texture| {
            let (w, h, rgba) = model::skin_front(texture.pixels(), texture.width(), texture.height(), sh.skin_slim.get(), 8)?;
            aegle::ui::scene::Image::new(w, h, rgba).ok()
        });
        match figure {
            Some(image) => {
                sh.skin_preview.set_image(&image);
                put(&sh.skin_shown, true);
            }
            None => put(&sh.skin_shown, false),
        }
    }

    pub(super) fn change_name(&self) {
        let name = self.shell.new_name.text().trim().to_string();
        if !Account::validate_username(&name) {
            put(&self.shell.account_status, "玩家名需为 3–16 位字母、数字或下划线".into());
            return;
        }
        if name == self.shell.account_title.get() {
            return;
        }
        self.profile_op("正在修改玩家名…", move |token| profile::change_name(token, &name));
    }

    /// Switches between the classic and slim arm model, keeping the current skin image.
    pub(super) fn set_skin_model(&self, slim: bool) {
        if self.shell.skin_slim.get() == slim {
            return;
        }
        put(&self.shell.skin_slim, slim);
        self.profile_op("正在切换皮肤模型…", move |token| {
            let current = profile::fetch_profile(token)?;
            let Some(skin) = current.active_skin() else {
                return Err(AuthError::Service("默认皮肤不能切换模型，请先上传皮肤".into()));
            };
            profile::set_skin_url(token, &skin.url, slim)
        });
    }

    pub(super) fn upload_skin(&self) {
        let path = self.shell.skin_path.path().trim().trim_matches('"').to_string();
        if path.is_empty() {
            put(&self.shell.account_status, "请填写皮肤 PNG 文件的路径".into());
            self.shell.skin_path.field().focus();
            return;
        }
        let png = match fs::read(&path) {
            Ok(png) => png,
            Err(e) => return put(&self.shell.account_status, format!("无法读取 {path}：{e}")),
        };
        if let Err(e) = profile::validate_skin_png(&png) {
            return put(&self.shell.account_status, e);
        }
        let slim = self.shell.skin_slim.get();
        self.profile_op("正在上传皮肤…", move |token| profile::upload_skin(token, &png, slim));
    }

    pub(super) fn reset_skin(&self) {
        self.profile_op("正在恢复默认皮肤…", profile::reset_skin);
    }

    pub(super) fn set_cape(&self, id: &str) {
        let id = (!id.is_empty()).then(|| id.to_string());
        self.profile_op("正在更换披风…", move |token| profile::set_cape(token, id.as_deref()));
    }
}
