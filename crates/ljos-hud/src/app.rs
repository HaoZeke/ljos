//! iced daemon: overlay, pop-out, summon, guest xdg-activation.

use std::time::Duration;

use iced::keyboard::{self, key::Named, Key};
use iced::window;
use iced::{event, time, Element, Event, Font, Pixels, Subscription, Task};

use crate::data::{unix_now, work_bin_mtime, Snapshot, WatchStamp};
use crate::install_desktop::{APP_ID as POPOUT_APP_ID, OVERLAY_APP_ID};
use crate::summon::{self, SummonAction, SummonRequest, SummonServer};
use crate::theme;
use crate::view::{self, Pane};

const HUD_W: f32 = 1100.0;
const HUD_H: f32 = 720.0;

/// First-paint inputs.
pub struct BootOpts {
    /// Whether the overlay starts mapped.
    pub visible: bool,
    /// Bound summon socket, held for process lifetime.
    pub summon: Option<SummonServer>,
}

/// iced messages.
#[derive(Debug, Clone)]
pub enum Message {
    /// 50 ms poll: summon socket, tray quit, place retry, clock chrome.
    Tick,
    /// Compare `work.bin` and pack `last_write_ts`; load only on a change.
    Watch,
    /// Off-thread habitat snapshot. Boxed so the message enum stays small.
    Snap(Box<Snapshot>),
    /// Kick an off-thread load.
    Refresh,
    Key(Key),
    /// Skip-chip or number-key pane focus.
    Pane(view::Pane),
    /// Island cue draft. Enter activates a read, not a write.
    CueChanged(String),
    /// Activate `packset_search` + `packset_island(cue, false)`.
    CueActivate,
    Close(window::Id),
    Closed(window::Id),
    WindowId(Option<window::Id>),
    /// Retry guest xdg-activation while a token is pending.
    ActivateRetry(u8),
    /// Clear the token only on success.
    ActivationApplied(bool),
}

/// iced application state.
pub struct HudApp {
    snap: Snapshot,
    pane: Pane,
    selected: usize,
    visible: bool,
    window_id: Option<window::Id>,
    opening: Option<window::Id>,
    popout_id: Option<window::Id>,
    opening_popout: Option<window::Id>,
    pending_activation_token: Option<String>,
    tray_quit: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    place_tries: u8,
    _summon: Option<SummonServer>,
    now: String,
    now_unix: u64,
    watch: WatchStamp,
    /// Island cue draft. Live cue is `snap.cue` after enter.
    cue: String,
}

impl HudApp {
    fn new(visible: bool, summon: Option<SummonServer>) -> Self {
        Self {
            snap: Snapshot::banner_only(String::new()),
            pane: Pane::Due,
            selected: 0,
            visible,
            window_id: None,
            opening: None,
            popout_id: None,
            opening_popout: None,
            pending_activation_token: if visible {
                summon::take_env_token()
            } else {
                let _ = summon::take_env_token();
                None
            },
            tray_quit: None,
            place_tries: 0,
            _summon: summon,
            now: ljos_cli::now_utc(),
            now_unix: unix_now(),
            watch: WatchStamp::default(),
            cue: String::new(),
        }
    }

    fn clamp_selected(&mut self) {
        let n = match self.pane {
            Pane::Due => self.snap.due.len(),
            Pane::Claims => self.snap.claims.len(),
            Pane::Trust => self.snap.graph.nodes.len(),
            Pane::Island => self.snap.island.len().saturating_add(self.snap.hits.len()),
            Pane::Deeds => self.snap.events.len().saturating_add(1),
        };
        if n == 0 {
            self.selected = 0;
        } else if self.selected >= n {
            self.selected = n - 1;
        }
    }

    fn mapped(&self) -> bool {
        self.window_id.is_some()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowId(id) => {
                if id.is_some() && id == self.opening_popout {
                    self.opening_popout = None;
                    return Task::none();
                }
                if id != self.opening {
                    return Task::none();
                }
                self.opening = None;
                self.window_id = id;
                if self.visible {
                    self.place_tries = 20;
                    self.try_place();
                    return self.apply_activation(0);
                }
                Task::none()
            }
            Message::Tick => {
                self.now = ljos_cli::now_utc();
                self.now_unix = unix_now();
                if self
                    .tray_quit
                    .as_ref()
                    .is_some_and(|q| q.load(std::sync::atomic::Ordering::Relaxed))
                {
                    return iced::exit();
                }
                self.try_place();
                if let Some(req) = summon::try_recv() {
                    return self.on_summon(req);
                }
                if work_bin_mtime() != self.watch.work_mtime {
                    return load_snap_with(self.keep_cue());
                }
                Task::none()
            }
            Message::Watch => {
                let stamp = WatchStamp::read();
                let load = self.watch.needs_load(&stamp);
                self.watch = stamp;
                if load {
                    return load_snap_with(self.keep_cue());
                }
                Task::none()
            }
            Message::Refresh => load_snap_with(self.keep_cue()),
            Message::CueChanged(s) => {
                self.cue = s;
                Task::none()
            }
            Message::CueActivate => {
                let live = self.cue.trim().to_string();
                self.snap.cue = live.clone();
                load_snap_with(if live.is_empty() { None } else { Some(live) })
            }
            Message::Snap(snap) => {
                self.snap = *snap;
                self.watch = WatchStamp::read();
                self.clamp_selected();
                Task::none()
            }
            Message::ActivateRetry(attempt) => self.apply_activation(attempt),
            Message::ActivationApplied(ok) => {
                if ok {
                    self.pending_activation_token = None;
                }
                Task::none()
            }
            Message::Close(id) => {
                if Some(id) == self.popout_id {
                    self.popout_id = None;
                    self.opening_popout = None;
                    self.visible = false;
                    return window::close(id);
                }
                self.hide()
            }
            Message::Closed(id) => {
                if Some(id) == self.popout_id {
                    self.popout_id = None;
                    self.visible = false;
                    self.pending_activation_token = None;
                    return Task::none();
                }
                self.opening = None;
                self.window_id = None;
                if self.popout_id.is_none() && self.opening_popout.is_none() {
                    self.visible = false;
                    self.pending_activation_token = None;
                }
                Task::none()
            }
            Message::Key(key) => self.on_key(key),
            Message::Pane(pane) => {
                self.pane = pane;
                self.selected = 0;
                Task::none()
            }
        }
    }

    fn keep_cue(&self) -> Option<String> {
        if self.snap.cue.is_empty() {
            None
        } else {
            Some(self.snap.cue.clone())
        }
    }

    fn on_summon(&mut self, req: SummonRequest) -> Task<Message> {
        let SummonRequest { action, token } = req;
        if self.summon_hides(action) {
            self.pending_activation_token = None;
        } else {
            self.pending_activation_token = token;
        }
        match action {
            SummonAction::Show => self.show(),
            SummonAction::Hide => self.hide(),
            SummonAction::Toggle => {
                if self.visible {
                    self.hide()
                } else {
                    self.show()
                }
            }
        }
    }

    fn summon_hides(&self, action: SummonAction) -> bool {
        matches!(action, SummonAction::Hide)
            || (matches!(action, SummonAction::Toggle) && self.visible)
    }

    fn show(&mut self) -> Task<Message> {
        self.visible = true;
        let open = self.sync_window();
        Task::batch([open, self.apply_activation(0)])
    }

    fn hide(&mut self) -> Task<Message> {
        self.visible = false;
        self.pending_activation_token = None;
        self.sync_window()
    }

    fn sync_window(&mut self) -> Task<Message> {
        if let Some(pop) = self.popout_id {
            if self.visible {
                return Task::none();
            }
            self.popout_id = None;
            self.opening_popout = None;
            return window::close(pop);
        }
        match overlay_action(self.visible, self.mapped()) {
            OverlayAction::Open => self.open_overlay(),
            OverlayAction::Close => {
                self.place_tries = 0;
                self.opening = None;
                match self.window_id.take() {
                    Some(id) => window::close(id),
                    None => Task::none(),
                }
            }
            OverlayAction::Place => {
                self.place_tries = 20;
                self.try_place();
                Task::none()
            }
            OverlayAction::Idle => Task::none(),
        }
    }

    fn open_overlay(&mut self) -> Task<Message> {
        self.place_tries = 20;
        let (id, open) = window::open(overlay_window());
        self.window_id = Some(id);
        self.opening = Some(id);
        open.map(|id| Message::WindowId(Some(id)))
    }

    /// Open a decorated pop-out and take the overlay down. A second request
    /// while one is open does nothing.
    fn pop_out(&mut self) -> Task<Message> {
        if self.popout_id.is_some() {
            return Task::none();
        }
        let mut tasks = Vec::new();
        self.place_tries = 0;
        self.opening = None;
        if let Some(id) = self.window_id.take() {
            tasks.push(window::close(id));
        }
        self.visible = true;
        let (id, open) = window::open(popout_window());
        self.popout_id = Some(id);
        self.opening_popout = Some(id);
        tasks.push(open.map(|id| Message::WindowId(Some(id))));
        Task::batch(tasks)
    }

    fn apply_activation(&self, attempt: u8) -> Task<Message> {
        let Some(id) = self.window_id else {
            return Task::none();
        };
        if !self.visible {
            return Task::none();
        }
        #[cfg(target_os = "linux")]
        {
            // Retry while the token is still pending: attempt 0 often runs
            // before iced has Wayland handles. Clear on success only
            // (`ActivationApplied(true)`). Tray / token-less toggle must
            // not steal the keyboard.
            let activate = match self.pending_activation_token.clone() {
                Some(tok) => window::run(id, move |win| crate::wlactivate::activate(win, &tok))
                    .map(Message::ActivationApplied),
                None => Task::none(),
            };
            if self.pending_activation_token.is_some() && attempt < 6 {
                return Task::batch([activate, delayed_activate(attempt.saturating_add(1))]);
            }
            activate
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (id, attempt);
            Task::none()
        }
    }

    fn try_place(&mut self) {
        if self.place_tries == 0 {
            return;
        }
        if crate::place::place_overlay() || !crate::place::sway_available() {
            self.place_tries = 0;
        } else {
            self.place_tries = self.place_tries.saturating_sub(1);
        }
    }

    fn on_key(&mut self, key: Key) -> Task<Message> {
        let ch = match &key {
            Key::Character(c) => Some(c.as_str()),
            _ => None,
        };
        match (&key, ch) {
            (Key::Named(Named::Tab), _) | (_, Some("l")) => {
                self.pane = self.pane.next();
                self.selected = 0;
            }
            (_, Some("h")) => {
                self.pane = self.pane.prev();
                self.selected = 0;
            }
            (Key::Named(Named::ArrowDown), _) | (_, Some("j")) => {
                if self.pane == Pane::Trust {
                    self.selected = self.snap.graph.walk_next(self.selected);
                } else {
                    self.selected = self.selected.saturating_add(1);
                    self.clamp_selected();
                }
            }
            (Key::Named(Named::ArrowUp), _) | (_, Some("k")) => {
                if self.pane == Pane::Trust {
                    self.selected = self.snap.graph.walk_prev(self.selected);
                } else {
                    self.selected = self.selected.saturating_sub(1);
                }
            }
            (_, Some("1")) => {
                self.pane = Pane::Due;
                self.selected = 0;
            }
            (_, Some("2")) => {
                self.pane = Pane::Claims;
                self.selected = 0;
            }
            (_, Some("3")) => {
                self.pane = Pane::Trust;
                self.selected = 0;
            }
            (_, Some("4")) => {
                self.pane = Pane::Island;
                self.selected = 0;
            }
            (_, Some("5")) => {
                self.pane = Pane::Deeds;
                self.selected = 0;
            }
            (_, Some("P") | Some("p")) => return self.pop_out(),
            (_, Some("r")) => return load_snap_with(self.keep_cue()),
            (Key::Named(Named::Escape), _) | (_, Some("q")) => return self.hide(),
            _ => {}
        }
        Task::none()
    }
}

/// The pop-out: a decorated window at the compositor's own level.
pub fn popout_window() -> window::Settings {
    let boot = icedtea::app::Boot::new("ljos", POPOUT_APP_ID)
        .decorations(true)
        .size(HUD_W, HUD_H)
        .min_size(360.0, 420.0);
    icedtea::app::bootstrap(&boot).window
}

/// Undecorated always-on-top overlay. Sway is told to float it over IPC.
pub fn overlay_window() -> window::Settings {
    let boot = icedtea::app::Boot::new("ljos", OVERLAY_APP_ID)
        .overlay()
        .size(HUD_W, HUD_H)
        .min_size(360.0, 420.0);
    icedtea::app::bootstrap(&boot).window
}

/// First paint, tray, then the iced daemon.
pub fn run(opts: BootOpts) -> anyhow::Result<()> {
    crate::log::info(&format!("hud start log={}", crate::log::path().display()));
    run_iced(opts).map_err(|err| anyhow::anyhow!("{err}"))
}

fn run_iced(opts: BootOpts) -> iced::Result {
    icedtea::typo::install_platform_faces();
    let cell = std::sync::Mutex::new(Some(opts));
    iced::daemon(
        move || {
            let opts = cell.lock().expect("boot").take().expect("boot once");
            boot(opts)
        },
        update,
        view,
    )
    .subscription(subscription)
    .theme(|_: &HudApp, _| theme::theme())
    .title(|_: &HudApp, _| "ljos".to_string())
    .default_font(icedtea::typo::UI)
    .settings(iced::Settings {
        default_text_size: Pixels::from(icedtea::typo::BODY),
        default_font: icedtea::typo::UI,
        ..Default::default()
    })
    .run()
}

fn boot(opts: BootOpts) -> (HudApp, Task<Message>) {
    let mut app = HudApp::new(opts.visible, opts.summon);
    app.tray_quit = crate::tray::start();
    let open = if app.visible {
        app.open_overlay()
    } else {
        Task::none()
    };
    (app, Task::batch([open, load_snap_with(None)]))
}

fn update(app: &mut HudApp, message: Message) -> Task<Message> {
    app.update(message)
}

fn view(app: &HudApp, _id: window::Id) -> Element<'_, Message> {
    view::view(
        &app.snap,
        app.pane,
        app.selected,
        &app.now,
        app.now_unix,
        &app.cue,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlayAction {
    Open,
    Close,
    Place,
    Idle,
}

fn overlay_action(visible: bool, mapped: bool) -> OverlayAction {
    match (visible, mapped) {
        (true, false) => OverlayAction::Open,
        (false, true) => OverlayAction::Close,
        (true, true) => OverlayAction::Place,
        (false, false) => OverlayAction::Idle,
    }
}

fn load_snap_with(cue: Option<String>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || Snapshot::load_with_cue(cue.as_deref()))
                .await
                .unwrap_or_else(|e| Snapshot::banner_only(format!("load: {e}")))
        },
        |snap| Message::Snap(Box::new(snap)),
    )
}

fn delayed_activate(attempt: u8) -> Task<Message> {
    let wait_ms = if attempt == 0 {
        30
    } else {
        40 + u64::from(attempt) * 20
    };
    Task::perform(
        async move {
            tokio::time::sleep(Duration::from_millis(wait_ms)).await;
        },
        move |_| Message::ActivateRetry(attempt),
    )
}

fn subscription(_app: &HudApp) -> Subscription<Message> {
    let keys = event::listen_with(|event, status, id| match event {
        Event::Window(window::Event::CloseRequested) => Some(Message::Close(id)),
        Event::Window(window::Event::Closed) => Some(Message::Closed(id)),
        Event::Keyboard(keyboard::Event::KeyPressed { key, .. })
            if status != event::Status::Captured =>
        {
            Some(Message::Key(key))
        }
        _ => None,
    });
    let tick = time::every(Duration::from_millis(50)).map(|_| Message::Tick);
    let watch = time::every(Duration::from_secs(2)).map(|_| Message::Watch);
    Subscription::batch([keys, tick, watch])
}

const _: Font = theme::FACE;

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_app() -> HudApp {
        let _guard = crate::env_lock();
        HudApp::new(true, None)
    }

    #[test]
    fn hide_closes_the_overlay_window() {
        assert_eq!(overlay_action(false, true), OverlayAction::Close);
        assert_eq!(overlay_action(true, false), OverlayAction::Open);
        assert_eq!(overlay_action(true, true), OverlayAction::Place);
        assert_eq!(overlay_action(false, false), OverlayAction::Idle);
        let src = include_str!("app.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        assert!(prod.contains("window::close"));
        assert!(prod.contains("iced::daemon"));
        assert!(prod.contains("window::Event::Closed"));
        assert!(prod.contains("wlactivate::activate"));
        assert!(prod.contains("ActivationApplied"));
        assert!(prod.contains("Message::Watch"));
        assert!(prod.contains("work_bin_mtime"));
        assert!(
            !prod.contains("let _ = tok"),
            "must not drop the activation token"
        );
        assert!(
            !prod.contains("window::gain_focus"),
            "token-less must not steal focus"
        );
        assert!(
            !prod.contains("map(|_| Message::Refresh)"),
            "tick/watch must not Snapshot::load on a timer"
        );
    }

    #[test]
    fn tick_is_chrome_not_a_snapshot_load() {
        let src = include_str!("app.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        assert!(prod.contains("self.now = ljos_cli::now_utc()"));
        assert!(prod.contains("self.now_unix = unix_now()"));
        assert!(prod.contains("WatchStamp::read()"));
        assert!(prod.contains("load_snap_with"));
        assert!(
            prod.contains("self.watch = WatchStamp::read()"),
            "Snap must stamp pack_ts, not only work.bin mtime"
        );
        assert!(
            !prod.contains("self.watch.work_mtime = work_bin_mtime()"),
            "Snap must not leave pack_ts as the Default empty string"
        );
        assert!(prod.contains("needs_load"));
    }

    #[test]
    fn activation_applied_true_clears_token() {
        let mut app = empty_app();
        app.pending_activation_token = Some("tok".into());
        let _ = app.update(Message::ActivationApplied(false));
        assert_eq!(app.pending_activation_token.as_deref(), Some("tok"));
        let _ = app.update(Message::ActivationApplied(true));
        assert_eq!(app.pending_activation_token, None);
    }

    #[test]
    fn hide_clears_pending_token() {
        let mut app = empty_app();
        app.pending_activation_token = Some("tok".into());
        let _ = app.hide();
        assert_eq!(app.pending_activation_token, None);
        assert!(!app.visible);
    }

    #[test]
    fn pop_out_takes_the_overlay_down_and_its_close_hides_the_hud() {
        let mut app = empty_app();
        let id = window::Id::unique();
        app.opening = Some(id);
        let _ = app.update(Message::WindowId(Some(id)));
        assert!(app.mapped());
        let _ = app.pop_out();
        assert!(!app.mapped(), "the overlay goes down");
        let pop = app.popout_id.expect("a pop-out window");
        assert!(app.visible);
        let _ = app.pop_out();
        assert_eq!(app.popout_id, Some(pop));
        let _ = app.update(Message::Close(pop));
        assert!(app.popout_id.is_none());
        assert!(!app.visible, "closing the pop-out hides, process stays");
    }

    #[test]
    fn summon_show_keeps_token_hide_drops_it() {
        let mut app = empty_app();
        app.visible = false;
        let _ = app.on_summon(SummonRequest {
            action: SummonAction::Show,
            token: Some("tok-1".into()),
        });
        assert_eq!(app.pending_activation_token.as_deref(), Some("tok-1"));
        let _ = app.on_summon(SummonRequest::new(SummonAction::Hide));
        assert_eq!(app.pending_activation_token, None);
    }

    #[test]
    fn visible_boot_stashes_env_token() {
        let _guard = crate::env_lock();
        #[allow(unused_unsafe)]
        unsafe {
            std::env::set_var("XDG_ACTIVATION_TOKEN", "tok-boot");
            std::env::set_var("DESKTOP_STARTUP_ID", "startup");
        }
        let app = HudApp::new(true, None);
        assert_eq!(app.pending_activation_token.as_deref(), Some("tok-boot"));
        assert!(std::env::var("XDG_ACTIVATION_TOKEN").is_err());
        assert!(std::env::var("DESKTOP_STARTUP_ID").is_err());
    }

    #[test]
    fn hidden_boot_unsets_token_without_stashing() {
        let _guard = crate::env_lock();
        #[allow(unused_unsafe)]
        unsafe {
            std::env::set_var("XDG_ACTIVATION_TOKEN", "tok-hid");
            std::env::set_var("DESKTOP_STARTUP_ID", "startup");
        }
        let app = HudApp::new(false, None);
        assert_eq!(app.pending_activation_token, None);
        assert!(!app.visible);
        assert!(std::env::var("XDG_ACTIVATION_TOKEN").is_err());
        assert!(std::env::var("DESKTOP_STARTUP_ID").is_err());
    }

    #[test]
    fn overlay_closed_hides() {
        let mut app = empty_app();
        let id = window::Id::unique();
        app.opening = Some(id);
        app.pending_activation_token = Some("tok".into());
        let _ = app.update(Message::WindowId(Some(id)));
        assert!(app.visible);
        assert!(app.mapped());
        let _ = app.update(Message::Closed(id));
        assert!(!app.visible);
        assert!(!app.mapped());
        assert_eq!(app.pending_activation_token, None);
    }

    #[test]
    fn overlay_closed_during_pop_out_keeps_visible() {
        let mut app = empty_app();
        let overlay = window::Id::unique();
        app.opening = Some(overlay);
        let _ = app.update(Message::WindowId(Some(overlay)));
        let _ = app.pop_out();
        assert!(app.visible);
        assert!(app.popout_id.is_some());
        let _ = app.update(Message::Closed(overlay));
        assert!(app.visible, "pop-out is still the mapped surface");
        assert!(app.popout_id.is_some());
    }

    #[test]
    fn skip_chip_focuses_the_graph_and_j_walks_nodes() {
        use crate::graph::GraphLayout;
        use ljos_cli::{Persona, Trust};
        let mut app = empty_app();
        app.snap.graph = GraphLayout::from_pack(
            &[
                Persona {
                    runner: None,
                    name: "a".into(),
                    anchor: 0.2,
                    view: "v".into(),
                    entities: vec![],
                },
                Persona {
                    runner: None,
                    name: "b".into(),
                    anchor: 0.8,
                    view: "v".into(),
                    entities: vec![],
                },
            ],
            &[Trust {
                from: "a".into(),
                to: "b".into(),
                weight: 0.7,
                about: vec![],
            }],
        );
        let _ = app.update(Message::Pane(Pane::Trust));
        assert_eq!(app.pane, Pane::Trust);
        assert_eq!(app.selected, 0);
        let _ = app.update(Message::Key(Key::Character("j".into())));
        assert_eq!(app.selected, 1);
        let _ = app.update(Message::Key(Key::Character("j".into())));
        assert_eq!(app.selected, 0);
        let _ = app.update(Message::Key(Key::Character("k".into())));
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn number_keys_reach_island_and_deeds() {
        let mut app = empty_app();
        let _ = app.update(Message::Key(Key::Character("4".into())));
        assert_eq!(app.pane, Pane::Island);
        let _ = app.update(Message::Key(Key::Character("5".into())));
        assert_eq!(app.pane, Pane::Deeds);
        let _ = app.update(Message::Key(Key::Character("1".into())));
        assert_eq!(app.pane, Pane::Due);
    }

    #[test]
    fn cue_activate_is_a_read() {
        let mut app = empty_app();
        assert!(app.cue.is_empty());
        assert!(app.snap.cue.is_empty());
        let _ = app.update(Message::CueChanged("ljos-9ptd".into()));
        assert_eq!(app.cue, "ljos-9ptd");
        assert!(app.snap.cue.is_empty(), "draft is not a write");
        let _ = app.update(Message::CueActivate);
        assert_eq!(app.snap.cue, "ljos-9ptd");
        let src = include_str!("app.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        assert!(prod.contains("Status::Captured"));
        assert!(prod.contains("CueActivate"));
        assert!(!prod.contains("graded("));
        assert!(!prod.contains("fire: true"));
        assert!(!prod.contains("selection_cue"));
    }
}
