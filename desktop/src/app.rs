//! Application state and frame loop. Screens live in `ui/` and are `impl App`
//! blocks so they share this state without ceremony.

use crate::backend::{Backend, ConnState, Event, Tag};
use crate::edit::{BpEdit, RuleForm};
use crate::model::{FlowTable, PAGE};
use crate::platform::{self, Browser, Family, Prefs, SystemAppearance, TrustPlan};
use crate::theme::{Palette, Scheme, Style};
use egui::{Context, Id};
use proto::pb::{self, client_message::Body as Req, server_message::Body as Res};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Inspector,
    /// Messages held at breakpoints.
    Breakpoints,
    Rules,
    /// Phones and simulators running the in-app SDK.
    Devices,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Overview,
    Request,
    Response,
    Timing,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Overview, Tab::Request, Tab::Response, Tab::Timing];
    pub fn label(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Request => "Request",
            Tab::Response => "Response",
            Tab::Timing => "Timing",
        }
    }
}

pub enum Pending {
    Page {
        gen: u64,
        page: usize,
    },
    Detail(u64),
    Status,
    CaInfo,
    Capture,
    Clear,
    Export,
    Subscribe,
    Hosts,
    /// Any rule change or fetch: the core answers with the full rule state.
    Rules,
    /// Start of the pushed notices; the core answers with what is held now.
    Watch,
    /// Resume or abort a held message; the core answers with what is left.
    Resume,
    Approvals,
    Mcp,
    /// A replay was sent; the answer names the new flow.
    Replay,
    /// The state of the SDK listener.
    SdkStatus,
    /// Start of the pushed device list; the core answers with the first one.
    DevicesSub,
    /// Allow, deny, revoke or a settings change.
    DeviceOp,
    /// A lazy SDK body was requested for this flow.
    FetchBody(u64),
}

pub struct Toast {
    pub text: String,
    pub good: bool,
    pub born: f64,
}

pub enum Sheet {
    ConfirmClear,
    ConfirmTrust(TrustPlan),
    /// SSL proxying hosts and Map Local rules.
    Proxy,
    /// Replace the SDK listener's certificate (every pairing is redone).
    ConfirmRegenCert,
}

pub enum Job {
    Trust(platform::CommandResult),
    Browser(Result<String, String>),
    Saved(Result<PathBuf, String>),
    /// A file was chosen for a Map Local rule.
    Mapped(pb::MapLocalRule),
}

/// A flow detail plus decoded textures and line caches.
pub struct DetailEntry {
    pub detail: pb::FlowDetail,
    pub req_lines: Option<Arc<Vec<String>>>,
    pub res_lines: Option<Arc<Vec<String>>>,
    pub req_tex: Option<egui::TextureHandle>,
    pub res_tex: Option<egui::TextureHandle>,
    /// What the proxy saw, for a merged flow.
    pub wire: Option<Box<DetailEntry>>,
}

pub struct App {
    pub backend: Box<dyn Backend>,
    pub sys: SystemAppearance,
    pub prefs: Prefs,
    pub st: Style,
    /// Eased light/dark switch: palette we are leaving, the one we reach,
    /// and when it began.
    pub theme_from: Option<Palette>,
    pub theme_to: Option<Palette>,
    pub theme_t0: f64,
    booted: bool,

    pub conn: ConnState,
    pub status: Option<pb::StatusResponse>,
    pub ca: Option<pb::CaInfo>,
    pub recording: bool,
    pub pending: HashMap<Tag, Pending>,

    pub section: Section,
    pub table: FlowTable,
    pub search: String,
    pub focus_search: bool,
    pub selected: Option<u64>,
    pub tab: Tab,
    pub details: HashMap<u64, DetailEntry>,
    pub detail_fetched_state: HashMap<u64, i32>,
    pub raw_view: bool,
    /// HTML bodies: show highlighted source instead of the preview.
    pub html_source: bool,
    /// Header lists in the Request and Response tabs.
    pub headers_open: bool,
    pub folds: HashMap<(u64, u8), std::collections::HashSet<usize>>,

    // hosts and focus
    /// Hosts the list is narrowed to. Empty means all traffic.
    pub focus: Vec<String>,
    pub hosts: Vec<pb::HostSummary>,
    pub hosts_stale: bool,
    pub hosts_next_fetch: f64,
    /// The collapsed "other hosts" group is open.
    pub others_open: bool,

    // breakpoints, approvals, AI access
    /// Messages held at breakpoints right now, oldest first.
    pub paused: Vec<pb::PausedFlow>,
    pub bp_selected: Option<u64>,
    pub bp_edit: Option<BpEdit>,
    /// Changes an AI client is waiting on.
    pub approvals: Vec<pb::Approval>,
    pub mcp: Option<pb::McpInfo>,
    pub show_token: bool,
    /// Rule editor: `None` when closed.
    pub rule_form: Option<RuleForm>,

    // in-app SDK
    pub sdk: Option<pb::SdkStatus>,
    pub devices: pb::DevicesSnapshot,
    /// Settings drawer of one device and app, when open.
    pub device_drawer: Option<crate::edit::DeviceForm>,
    pub hints_open: bool,
    /// The connection status popover under the toolbar icon.
    pub link_open: bool,
    /// Where the toolbar icon was drawn last frame, so the popover can sit under it.
    pub link_rect: Option<egui::Rect>,
    /// When to ask the core for the SDK status again (egui time, seconds).
    pub sdk_next_poll: f64,
    /// Devices and apps the Inspector is narrowed to (filter chips).
    pub device_filter: Vec<String>,
    pub app_filter: Vec<String>,
    /// Flows whose lazy body is being fetched.
    pub fetching: std::collections::HashSet<(u64, bool)>,
    /// Which timings view a merged flow shows.
    pub wire_view: bool,
    /// Optional table columns, from the header's context menu.
    pub col_name: bool,
    pub col_screen: bool,
    pub col_route: bool,

    // rules
    pub rules: pb::RulesResponse,
    pub proxy_tab: usize,
    pub ssl_input: String,
    pub exclude_input: String,

    // table view
    pub follow: bool,
    pub last_total: usize,
    pub scroll_to: Option<f32>,
    pub view_offset: f32,
    pub view_height: f32,
    pub unseen: usize,

    // layout
    pub detail_raw: f32,

    // overlays
    pub toasts: Vec<Toast>,
    pub sheet: Option<Sheet>,
    pub sheet_ghost: Option<Sheet>,

    // settings
    pub family: Family,
    pub browsers: Vec<Browser>,
    pub trusted_system: Option<bool>,
    pub trusted_nss: Option<bool>,
    pub spki_launch: bool,
    pub redact_export: bool,
    pub busy: Option<&'static str>,
    pub job_tx: mpsc::Sender<Job>,
    pub job_rx: mpsc::Receiver<Job>,
    pub last_trust_check: f64,
    pub automation: Option<crate::automation::Automation>,
}

impl App {
    pub fn new(
        ctx: &Context,
        backend: Box<dyn Backend>,
        sys: SystemAppearance,
        prefs: Prefs,
    ) -> App {
        let (job_tx, job_rx) = mpsc::channel();
        let st = Style {
            p: Palette::new(sys.scheme(), sys.high_contrast()),
            reduced_motion: sys.reduce_motion(),
            reduced_transparency: sys.high_contrast(),
        };
        let mut app = App {
            backend,
            sys,
            detail_raw: prefs.detail_width,
            prefs,
            st,
            theme_from: None,
            theme_to: None,
            theme_t0: 0.0,
            booted: false,
            conn: ConnState::Starting,
            status: None,
            ca: None,
            recording: true,
            pending: HashMap::new(),
            section: Section::Inspector,
            table: FlowTable::new(),
            search: String::new(),
            focus_search: false,
            selected: None,
            tab: Tab::Overview,
            details: HashMap::new(),
            detail_fetched_state: HashMap::new(),
            raw_view: false,
            html_source: false,
            headers_open: true,
            folds: HashMap::new(),
            focus: Vec::new(),
            hosts: Vec::new(),
            hosts_stale: true,
            hosts_next_fetch: 0.0,
            others_open: false,
            paused: Vec::new(),
            bp_selected: None,
            bp_edit: None,
            approvals: Vec::new(),
            mcp: None,
            show_token: false,
            rule_form: None,
            sdk: None,
            devices: pb::DevicesSnapshot::default(),
            device_drawer: None,
            hints_open: false,
            link_open: false,
            link_rect: None,
            sdk_next_poll: 0.0,
            device_filter: Vec::new(),
            app_filter: Vec::new(),
            fetching: Default::default(),
            wire_view: false,
            col_name: true,
            col_screen: false,
            col_route: false,
            rules: pb::RulesResponse::default(),
            proxy_tab: 0,
            ssl_input: String::new(),
            exclude_input: String::new(),
            follow: true,
            last_total: 0,
            scroll_to: None,
            view_offset: 0.0,
            view_height: 0.0,
            unseen: 0,
            toasts: Vec::new(),
            sheet: None,
            sheet_ghost: None,
            family: platform::detect_family(),
            browsers: platform::detect_browsers(),
            trusted_system: None,
            trusted_nss: None,
            spki_launch: false,
            redact_export: true,
            busy: None,
            job_tx,
            job_rx,
            last_trust_check: -100.0,
            automation: crate::automation::Automation::from_env(),
        };
        app.sync_style(ctx);
        app.booted = true;
        app
    }

    /// Resolve theme and motion preferences into the active style.
    pub fn sync_style(&mut self, ctx: &Context) {
        let scheme = match self.prefs.theme.as_str() {
            "light" => Scheme::Light,
            "dark" => Scheme::Dark,
            _ => self.sys.scheme(),
        };
        let hc = self.sys.high_contrast();
        let target = Palette::new(scheme, hc);
        let reduced = self
            .prefs
            .reduce_motion
            .unwrap_or_else(|| self.sys.reduce_motion());
        if self.booted && !reduced && self.st.p.scheme != target.scheme {
            // Ease between schemes instead of jumping in brightness.
            self.theme_from = Some(self.st.p.clone());
            self.theme_to = Some(target.clone());
            self.theme_t0 = ctx.input(|i| i.time);
            self.st.p.scheme = target.scheme;
            self.st.reduced_motion = reduced;
            self.st.reduced_transparency = hc;
        } else {
            self.theme_from = None;
            self.theme_to = None;
            self.st = Style {
                p: target,
                reduced_motion: reduced,
                reduced_transparency: hc,
            };
        }
        crate::style::apply(ctx, &self.st);
    }

    /// Advance the light/dark transition by one frame.
    pub fn step_theme(&mut self, ctx: &Context) {
        let (Some(from), Some(to)) = (&self.theme_from, &self.theme_to) else {
            return;
        };
        let t = ((ctx.input(|i| i.time) - self.theme_t0) / 0.32).clamp(0.0, 1.0) as f32;
        let e = 1.0 - (1.0 - t).powi(3);
        self.st.p = Palette::lerp(from, to, e);
        if t >= 1.0 {
            self.theme_from = None;
            self.theme_to = None;
        } else {
            ctx.request_repaint();
        }
        crate::style::apply(ctx, &self.st);
    }

    // ------------------------------------------------------------ requests

    pub fn send(&mut self, body: Req, pending: Pending) -> Tag {
        let tag = self.backend.request(body);
        self.pending.insert(tag, pending);
        tag
    }

    pub fn current_filter(&self) -> pb::Filter {
        pb::Filter {
            kind: self.table.kind,
            text: self.search.clone(),
            hosts: self.focus.clone(),
            devices: self.device_filter.clone(),
            apps: self.app_filter.clone(),
        }
    }

    // ------------------------------------------------------------ SDK

    pub fn set_sdk_enabled(&mut self, enabled: bool) {
        self.send(
            Req::SdkSetEnabled(pb::SdkSetEnabledRequest { enabled }),
            Pending::SdkStatus,
        );
        if let Some(s) = &mut self.sdk {
            s.enabled = enabled;
        }
    }

    /// Change the remembered SDK settings. The answer is the new status.
    pub fn set_sdk_settings(&mut self, lan: bool, auto_accept_local: bool) {
        let enabled = self.sdk.as_ref().is_some_and(|s| s.enabled);
        self.send(
            Req::SdkSetSettings(pb::SdkSetSettingsRequest {
                enabled,
                lan,
                auto_accept_local,
            }),
            Pending::SdkStatus,
        );
        if let Some(s) = &mut self.sdk {
            s.lan = lan;
            s.auto_accept_local = auto_accept_local;
        }
    }

    /// Keep the connection checks current: adb, the firewall and discovery change without any push. Every few
    /// seconds, faster while the status popover is open, and never with a request already in flight.
    pub fn poll_sdk(&mut self, ctx: &Context) {
        if self.conn != ConnState::Connected || self.sdk.as_ref().is_some_and(|s| !s.available) {
            return;
        }
        let now = ctx.input(|i| i.time);
        if now < self.sdk_next_poll {
            ctx.request_repaint_after(std::time::Duration::from_millis(
                ((self.sdk_next_poll - now) * 1000.0) as u64 + 10,
            ));
            return;
        }
        self.sdk_next_poll = now + if self.link_open { 1.5 } else { 5.0 };
        if self.pending.values().any(|p| matches!(p, Pending::SdkStatus)) {
            return;
        }
        self.send(Req::SdkStatus(pb::SdkStatusRequest {}), Pending::SdkStatus);
    }

    pub fn allow_prompt(&mut self, id: u64) {
        self.device_op(pb::device_action_request::Kind::Allow, id, "", "");
        self.devices.pending.retain(|p| p.id != id);
    }

    pub fn deny_prompt(&mut self, id: u64) {
        self.device_op(pb::device_action_request::Kind::Deny, id, "", "");
        self.devices.pending.retain(|p| p.id != id);
    }

    pub fn revoke_device(&mut self, device_id: &str, app_id: &str) {
        self.device_op(
            pb::device_action_request::Kind::Revoke,
            0,
            device_id,
            app_id,
        );
    }

    pub fn forget_device(&mut self, device_id: &str) {
        self.device_op(pb::device_action_request::Kind::Forget, 0, device_id, "");
    }

    fn device_op(
        &mut self,
        kind: pb::device_action_request::Kind,
        id: u64,
        device: &str,
        app: &str,
    ) {
        self.send(
            Req::DeviceAction(pb::DeviceActionRequest {
                kind: kind as i32,
                pending_id: id,
                device_id: device.to_string(),
                app_id: app.to_string(),
            }),
            Pending::DeviceOp,
        );
    }

    pub fn save_device_config(&mut self, device_id: &str, app_id: &str, cfg: pb::DeviceConfig) {
        self.send(
            Req::SetDeviceConfig(pb::SetDeviceConfigRequest {
                device_id: device_id.to_string(),
                app_id: app_id.to_string(),
                config: Some(cfg),
            }),
            Pending::DeviceOp,
        );
    }

    pub fn regenerate_sdk_cert(&mut self) {
        self.send(
            Req::RegenerateSdkCert(pb::RegenerateSdkCertRequest {}),
            Pending::SdkStatus,
        );
    }

    /// Ask the device for a lazy body of the flow being viewed.
    pub fn fetch_body(&mut self, flow_id: u64, response: bool) {
        if !self.fetching.insert((flow_id, response)) {
            return;
        }
        self.send(
            Req::FetchBody(pb::FetchBodyRequest { flow_id, response }),
            Pending::FetchBody(flow_id),
        );
    }

    /// Narrow the Inspector to one device (empty clears).
    pub fn set_device_filter(&mut self, devices: Vec<String>, apps: Vec<String>) {
        if self.device_filter == devices && self.app_filter == apps {
            return;
        }
        self.device_filter = devices;
        self.app_filter = apps;
        self.filter_changed();
    }

    /// Narrow the list to these hosts (empty clears the focus).
    pub fn set_focus(&mut self, hosts: Vec<String>) {
        if self.focus == hosts {
            return;
        }
        self.focus = hosts;
        self.others_open = false;
        self.filter_changed();
    }

    /// Click focuses one host; a modifier click adds or removes it from the
    /// set; clicking the only focused host again clears the focus.
    pub fn click_host(&mut self, host: &str, multi: bool) {
        let has = self.focus.iter().any(|h| h == host);
        let next = if multi {
            let mut v = self.focus.clone();
            if has {
                v.retain(|h| h != host);
            } else {
                v.push(host.to_string());
            }
            v
        } else if has && self.focus.len() == 1 {
            Vec::new()
        } else {
            vec![host.to_string()]
        };
        self.set_focus(next);
    }

    pub fn fetch_hosts(&mut self) {
        self.send(Req::Hosts(pb::HostsRequest { limit: 0 }), Pending::Hosts);
    }

    pub fn fetch_rules(&mut self) {
        self.send(Req::Rules(pb::RulesRequest {}), Pending::Rules);
    }

    /// Decrypt (or tunnel) one host without touching the others.
    pub fn ssl_host(&mut self, ctx: &Context, host: &str, enable: bool) {
        self.send(
            Req::SslHost(pb::SslHostRequest {
                host: host.to_string(),
                enable,
            }),
            Pending::Rules,
        );
        let msg = if enable {
            format!("SSL proxying on for {host}. New connections are decrypted.")
        } else {
            format!("SSL proxying off for {host}. New connections are tunneled.")
        };
        self.toast(ctx, msg, true);
    }

    pub fn set_ssl(&mut self, ssl: pb::SslSettings) {
        self.send(
            Req::SetSsl(pb::SetSslRequest { ssl: Some(ssl) }),
            Pending::Rules,
        );
    }

    pub fn put_rule(&mut self, rule: pb::Rule) {
        self.send(
            Req::PutRule(pb::PutRuleRequest { rule: Some(rule) }),
            Pending::Rules,
        );
    }

    pub fn set_rule_enabled(&mut self, id: &str, enabled: bool) {
        self.send(
            Req::SetRuleEnabled(pb::SetRuleEnabledRequest {
                id: id.to_string(),
                enabled,
            }),
            Pending::Rules,
        );
    }

    /// Let a held message go, with the edits made in the editor.
    pub fn resume_held(&mut self, flow_id: u64) {
        let Some(orig) = self.paused.iter().find(|p| p.flow_id == flow_id).cloned() else {
            return;
        };
        let req = match &self.bp_edit {
            Some(e) if e.flow_id == flow_id => e.resume(&orig),
            _ => pb::ResumeRequest {
                flow_id,
                ..Default::default()
            },
        };
        self.send(Req::Resume(req), Pending::Resume);
        self.paused.retain(|p| p.flow_id != flow_id);
        self.close_editor_for(flow_id);
    }

    pub fn abort_held(&mut self, flow_id: u64) {
        self.send(
            Req::Resume(pb::ResumeRequest {
                flow_id,
                abort: true,
                abort_reason: "stopped in the Proxy app".into(),
                ..Default::default()
            }),
            Pending::Resume,
        );
        self.paused.retain(|p| p.flow_id != flow_id);
        self.close_editor_for(flow_id);
    }

    fn close_editor_for(&mut self, flow_id: u64) {
        if self.bp_selected == Some(flow_id) {
            self.bp_selected = self.paused.first().map(|p| p.flow_id);
            self.bp_edit = None;
        }
    }

    /// Select a held message and load it into the editor.
    pub fn select_held(&mut self, flow_id: Option<u64>) {
        self.bp_selected = flow_id;
        self.bp_edit = flow_id
            .and_then(|id| self.paused.iter().find(|p| p.flow_id == id))
            .map(BpEdit::new);
    }

    pub fn decide_approval(&mut self, id: &str, allow: bool) {
        self.send(
            Req::DecideApproval(pb::DecideApprovalRequest {
                id: id.to_string(),
                allow,
            }),
            Pending::Approvals,
        );
        self.approvals.retain(|a| a.id != id);
    }

    pub fn set_mcp_writes(&mut self, writes: pb::McpWrites) {
        if let Some(m) = &mut self.mcp {
            m.writes = writes as i32;
        }
        self.send(
            Req::SetMcpWrites(pb::SetMcpWritesRequest {
                writes: writes as i32,
            }),
            Pending::Mcp,
        );
    }

    /// Send a captured request again.
    pub fn replay(&mut self, id: u64) {
        self.send(
            Req::Replay(pb::ReplayRequest {
                id,
                ..Default::default()
            }),
            Pending::Replay,
        );
    }

    pub fn remove_rule(&mut self, id: &str) {
        self.send(
            Req::RemoveRule(pb::RemoveRuleRequest { id: id.to_string() }),
            Pending::Rules,
        );
    }

    /// Pick a file for a Map Local rule that answers this flow's URL.
    pub fn map_local_pick(&mut self, ctx: &Context, s: &pb::FlowSummary) {
        let (path, _query) = s.path.split_once('?').unwrap_or((&s.path, ""));
        let rule = pb::MapLocalRule {
            id: String::new(),
            enabled: true,
            host: s.host.clone(),
            port: s.port,
            path: path.to_string(),
            query: String::new(),
            file: String::new(),
            status: 200,
            content_type: String::new(),
        };
        let tx = self.job_tx.clone();
        let c = ctx.clone();
        let title = format!("Choose the file to serve for {}{}", s.host, path);
        std::thread::spawn(move || {
            if let Some(file) = rfd::FileDialog::new().set_title(title).pick_file() {
                let mut rule = rule;
                rule.file = file.display().to_string();
                let _ = tx.send(Job::Mapped(rule));
                c.request_repaint();
            }
        });
    }

    pub fn resubscribe(&mut self) {
        let filter = self.current_filter();
        self.send(
            Req::Subscribe(pb::SubscribeRequest {
                filter: Some(filter),
            }),
            Pending::Subscribe,
        );
    }

    /// The filter or search text changed: new result set, new subscription.
    pub fn filter_changed(&mut self) {
        self.table.text = self.search.clone();
        self.table.invalidate();
        self.scroll_to = Some(0.0);
        self.follow = true;
        self.resubscribe();
    }

    pub fn request_page(&mut self, gen: u64, page: usize) {
        let filter = self.current_filter();
        self.send(
            Req::Search(pb::SearchRequest {
                filter: Some(filter),
                since_ms: 0,
                until_ms: 0,
                offset: (page * PAGE) as u32,
                limit: PAGE as u32,
            }),
            Pending::Page { gen, page },
        );
    }

    pub fn fetch_detail(&mut self, id: u64) {
        self.send(
            Req::GetFlow(pb::GetFlowRequest { id, parts: 7 }),
            Pending::Detail(id),
        );
    }

    pub fn select(&mut self, id: Option<u64>) {
        if self.selected == id {
            return;
        }
        self.selected = id;
        self.folds.clear();
        if let Some(id) = id {
            let known = self
                .table
                .row_of(id)
                .and_then(|r| self.table.get(r))
                .map(|s| s.state);
            let fresh = matches!(known, Some(s) if self.detail_fetched_state.get(&id) == Some(&s));
            if !(self.details.contains_key(&id) && fresh) {
                self.fetch_detail(id);
            }
        }
    }

    pub fn toggle_recording(&mut self) {
        let action = if self.recording {
            pb::capture_request::Action::Stop
        } else {
            pb::capture_request::Action::Start
        };
        // Optimistic: the control answers on pointer-down, the core confirms.
        self.recording = !self.recording;
        self.send(
            Req::Capture(pb::CaptureRequest {
                action: action as i32,
            }),
            Pending::Capture,
        );
    }

    pub fn clear_all(&mut self) {
        self.send(Req::Clear(pb::ClearRequest {}), Pending::Clear);
    }

    pub fn export_har(&mut self) {
        let filter = self.current_filter();
        self.send(
            Req::Export(pb::ExportRequest {
                kind: "har".into(),
                ids: vec![],
                filter: Some(filter),
                redact: self.redact_export,
            }),
            Pending::Export,
        );
    }

    pub fn toast(&mut self, ctx: &Context, text: impl Into<String>, good: bool) {
        let now = ctx.input(|i| i.time);
        self.toasts.push(Toast {
            text: text.into(),
            good,
            born: now,
        });
    }

    pub fn open_sheet(&mut self, s: Sheet) {
        self.sheet = Some(s);
    }

    pub fn close_sheet(&mut self) {
        if let Some(s) = self.sheet.take() {
            self.sheet_ghost = Some(s);
        }
    }

    // ------------------------------------------------------------ events

    fn on_connected(&mut self) {
        self.pending.clear();
        self.table.invalidate();
        self.resubscribe();
        self.send(Req::Status(pb::StatusRequest {}), Pending::Status);
        self.send(Req::CaInfo(pb::CaInfoRequest {}), Pending::CaInfo);
        self.fetch_rules();
        self.send(Req::Watch(pb::WatchRequest {}), Pending::Watch);
        self.send(
            Req::Approvals(pb::ListApprovalsRequest {}),
            Pending::Approvals,
        );
        self.send(Req::McpInfo(pb::McpInfoRequest {}), Pending::Mcp);
        self.send(Req::SdkStatus(pb::SdkStatusRequest {}), Pending::SdkStatus);
        self.send(
            Req::DevicesSubscribe(pb::DevicesSubscribeRequest {}),
            Pending::DevicesSub,
        );
        self.hosts_stale = true;
        self.send(
            Req::Capture(pb::CaptureRequest {
                action: pb::capture_request::Action::Status as i32,
            }),
            Pending::Capture,
        );
        if let Some(id) = self.selected {
            self.fetch_detail(id);
        }
    }

    pub fn pump(&mut self, ctx: &Context) {
        for ev in self.backend.drain() {
            match ev {
                Event::Conn(c) => {
                    let connected = c == ConnState::Connected;
                    let was = self.conn == ConnState::Connected;
                    self.conn = c;
                    if connected && !was {
                        self.on_connected();
                    } else if !connected {
                        self.pending.clear();
                    }
                }
                Event::Batch(b) => {
                    let before = self.table.total();
                    let effect = self.table.apply_batch(&b);
                    if effect.cleared {
                        self.selected = None;
                        self.details.clear();
                        self.detail_fetched_state.clear();
                        self.unseen = 0;
                        self.hosts.clear();
                        if !self.focus.is_empty() {
                            self.focus.clear();
                            self.filter_changed();
                        }
                    }
                    if !b.upserts.is_empty() || effect.cleared {
                        self.hosts_stale = true;
                    }
                    // A flow we are looking at moved on: refresh its detail.
                    if let Some(sel) = self.selected {
                        if let Some(u) = b.upserts.iter().find(|u| u.id == sel) {
                            if self.detail_fetched_state.get(&sel) != Some(&u.state) {
                                self.fetch_detail(sel);
                            }
                        }
                    }
                    let _ = before;
                }
                Event::Resync => self.table.invalidate(),
                Event::Devices(d) => self.on_devices(ctx, d),
                Event::Pause(n) => self.on_pause_notice(ctx, n),
                Event::Approval(n) => {
                    if let Some(a) = n.requested {
                        if !self.approvals.iter().any(|x| x.id == a.id) {
                            self.approvals.push(a);
                        }
                    }
                    if !n.decided_id.is_empty() {
                        self.approvals.retain(|a| a.id != n.decided_id);
                    }
                }
                Event::Response(tag, body) => self.on_response(ctx, tag, body),
            }
        }
        while let Ok(job) = self.job_rx.try_recv() {
            self.busy = None;
            match job {
                Job::Trust(r) => {
                    if r.ok {
                        self.toast(ctx, "Certificate installed", true);
                    } else {
                        let msg = if r.output.is_empty() {
                            "Installation was cancelled".to_string()
                        } else {
                            r.output
                        };
                        self.toast(ctx, msg, false);
                    }
                    self.last_trust_check = -100.0;
                }
                Job::Mapped(rule) => {
                    let name = std::path::Path::new(&rule.file)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.toast(
                        ctx,
                        format!("Mapped {}{} to {name}", rule.host, rule.path),
                        true,
                    );
                    self.send(
                        Req::PutMapLocal(pb::PutMapLocalRequest { rule: Some(rule) }),
                        Pending::Rules,
                    );
                }
                Job::Browser(Ok(msg)) => self.toast(ctx, msg, true),
                Job::Browser(Err(e)) => self.toast(ctx, e, false),
                Job::Saved(Ok(path)) => self.toast(ctx, format!("Saved {}", path.display()), true),
                Job::Saved(Err(e)) => {
                    if !e.is_empty() {
                        self.toast(ctx, e, false)
                    }
                }
            }
        }
    }

    /// A new device list. Say so when a pairing request arrives, because the
    /// person may be looking at another screen.
    fn on_devices(&mut self, ctx: &Context, d: pb::DevicesSnapshot) {
        for p in &d.pending {
            if !self.devices.pending.iter().any(|x| x.id == p.id) {
                let msg = format!(
                    "{} wants to pair. Code {} {}",
                    p.device_name,
                    &p.code[..p.code.len().min(3)],
                    p.code.get(3..).unwrap_or("")
                );
                self.toast(ctx, msg, true);
            }
        }
        // A device that was not connected a moment ago is: this is the "it just works" moment, so say so.
        if !self.devices.devices.is_empty() || self.devices.rev != 0 {
            for n in &d.devices {
                let connected = |x: &pb::Device| {
                    x.device_id == n.device_id
                        && x.app_id == n.app_id
                        && x.status == pb::DeviceStatus::DeviceConnected as i32
                };
                if n.status == pb::DeviceStatus::DeviceConnected as i32
                    && !self.devices.devices.iter().any(connected)
                {
                    let name = if n.app_name.is_empty() { &n.app_id } else { &n.app_name };
                    self.toast(ctx, format!("{} connected from {}", name, n.device_name), true);
                }
            }
        }
        self.devices = d;
        if self
            .sdk
            .as_ref()
            .is_some_and(|s| s.enabled != self.devices.enabled)
        {
            self.send(Req::SdkStatus(pb::SdkStatusRequest {}), Pending::SdkStatus);
        }
    }

    fn on_pause_notice(&mut self, ctx: &Context, n: pb::PauseNotice) {
        if let Some(p) = n.paused {
            let first = self.paused.is_empty();
            let id = p.flow_id;
            let label = format!(
                "{} {}{} is held",
                p.method,
                p.summary.as_ref().map(|s| s.host.as_str()).unwrap_or(""),
                p.path
            );
            self.paused.retain(|x| x.flow_id != id);
            self.paused.push(p);
            if first || self.bp_selected.is_none() {
                self.select_held(Some(id));
            }
            self.toast(ctx, label, true);
        }
        if n.released_flow_id != 0 {
            let id = n.released_flow_id;
            self.paused.retain(|p| p.flow_id != id);
            self.close_editor_for(id);
        }
    }

    /// Keep the host list fresh without asking more than once a second, and
    /// never while a request for it is already in flight.
    pub fn refresh_hosts(&mut self, ctx: &Context) {
        if !self.hosts_stale || self.conn != ConnState::Connected {
            return;
        }
        let now = ctx.input(|i| i.time);
        if now < self.hosts_next_fetch {
            ctx.request_repaint_after(std::time::Duration::from_millis(
                ((self.hosts_next_fetch - now) * 1000.0) as u64 + 10,
            ));
            return;
        }
        if self.pending.values().any(|p| matches!(p, Pending::Hosts)) {
            return;
        }
        self.hosts_stale = false;
        self.hosts_next_fetch = now + 1.0;
        self.fetch_hosts();
    }

    fn on_response(&mut self, ctx: &Context, tag: Tag, body: Res) {
        let Some(p) = self.pending.remove(&tag) else {
            return;
        };
        match (p, body) {
            (Pending::Page { gen, page }, Res::Search(r)) => self.table.on_page(gen, page, r),
            (Pending::Page { gen, page }, _) => self.table.on_page_failed(gen, page),
            (Pending::Detail(id), Res::Flow(d)) => {
                if let Some(s) = &d.summary {
                    self.detail_fetched_state.insert(id, s.state);
                }
                let entry = self.make_entry(ctx, d);
                self.details.insert(id, entry);
                if self.details.len() > 48 {
                    let keep = self.selected;
                    let victims: Vec<u64> = self
                        .details
                        .keys()
                        .copied()
                        .filter(|k| Some(*k) != keep)
                        .take(16)
                        .collect();
                    for v in victims {
                        self.details.remove(&v);
                        self.detail_fetched_state.remove(&v);
                    }
                }
            }
            (Pending::Hosts, Res::Hosts(h)) => self.hosts = h.hosts,
            (Pending::Rules, Res::Rules(r)) => {
                self.rules = r;
                // Whether a host is decrypted changed with the rules.
                self.hosts_stale = true;
            }
            (Pending::Watch, Res::Paused(l)) | (Pending::Resume, Res::Paused(l)) => {
                self.paused = l.items;
                let keep = self
                    .bp_selected
                    .filter(|id| self.paused.iter().any(|p| p.flow_id == *id));
                if keep.is_none() {
                    let first = self.paused.first().map(|p| p.flow_id);
                    self.select_held(first);
                }
            }
            (Pending::Approvals, Res::Approvals(a)) => self.approvals = a.pending,
            (Pending::SdkStatus, Res::SdkStatus(s)) => self.sdk = Some(s),
            (Pending::DevicesSub, Res::Devices(d)) => self.devices = d,
            (Pending::DevicesSub, Res::Error(_)) => {}
            (Pending::DeviceOp, Res::Ok(_)) => {}
            (Pending::FetchBody(id), Res::Flow(d)) => {
                self.fetching.retain(|(f, _)| *f != id);
                let entry = self.make_entry(ctx, d);
                self.details.insert(id, entry);
            }
            (Pending::FetchBody(id), Res::Error(e)) => {
                self.fetching.retain(|(f, _)| *f != id);
                self.toast(ctx, e.message, false);
            }
            (Pending::Mcp, Res::McpInfo(m)) => self.mcp = Some(m),
            (Pending::Replay, Res::Replay(r)) => {
                self.toast(
                    ctx,
                    format!("Sent again as flow {} ({})", r.flow_id, r.status),
                    true,
                );
                self.follow = true;
            }
            (Pending::Status, Res::Status(s)) => {
                self.recording = s.recording;
                self.status = Some(s);
            }
            (Pending::CaInfo, Res::CaInfo(c)) => self.ca = Some(c),
            (Pending::Capture, Res::Capture(c)) => self.recording = c.recording,
            (Pending::Clear, Res::Cleared(c)) => {
                self.toast(ctx, format!("Cleared {} flows", c.removed), true);
            }
            (Pending::Export, Res::Export(x)) => self.save_export(x),
            (_, Res::Error(e)) if e.code != "not_found" => self.toast(ctx, e.message, false),
            _ => {}
        }
    }

    fn make_entry(&self, ctx: &Context, mut d: pb::FlowDetail) -> DetailEntry {
        let wire = d.wire.take().map(|w| Box::new(self.make_entry(ctx, *w)));
        let lines = |b: &Option<pb::BodyView>| -> Option<Arc<Vec<String>>> {
            let b = b.as_ref()?;
            if b.text.is_empty() {
                return None;
            }
            Some(Arc::new(
                b.text
                    .lines()
                    .map(|l| {
                        if l.chars().count() > 2000 {
                            let cut: String = l.chars().take(2000).collect();
                            format!("{cut}… ({} more characters)", l.chars().count() - 2000)
                        } else {
                            l.to_string()
                        }
                    })
                    .collect(),
            ))
        };
        let tex = |b: &Option<pb::BodyView>, name: &str| -> Option<egui::TextureHandle> {
            let b = b.as_ref()?;
            if b.image.is_empty() {
                return None;
            }
            let img = image::load_from_memory(&b.image).ok()?.to_rgba8();
            let size = [img.width() as usize, img.height() as usize];
            let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
            Some(ctx.load_texture(name, color, egui::TextureOptions::LINEAR))
        };
        let id = d.summary.as_ref().map(|s| s.id).unwrap_or(0);
        DetailEntry {
            req_lines: lines(&d.request_body),
            res_lines: lines(&d.response_body),
            req_tex: tex(&d.request_body, &format!("req-{id}")),
            res_tex: tex(&d.response_body, &format!("res-{id}")),
            detail: d,
            wire,
        }
    }

    fn save_export(&mut self, x: pb::ExportResponse) {
        let tx = self.job_tx.clone();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_file_name(&x.filename)
                .add_filter("HTTP Archive", &["har"])
                .save_file();
            let result = match picked {
                Some(path) => std::fs::write(&path, &x.data)
                    .map(|_| path)
                    .map_err(|e| format!("Could not save: {e}")),
                None => Err(String::new()),
            };
            let _ = tx.send(Job::Saved(result));
        });
    }

    // ------------------------------------------------------------ helpers

    /// The detail being shown: for a merged flow, the App or the Wire half.
    pub fn view_entry(&self, id: u64) -> Option<&DetailEntry> {
        let e = self.details.get(&id)?;
        if self.wire_view {
            if let Some(w) = &e.wire {
                return Some(w);
            }
        }
        Some(e)
    }

    /// Jump to the settings of the device a flow came from.
    pub fn open_device_settings(&mut self, device_id: &str, app_id: &str) {
        self.section = Section::Devices;
        self.device_drawer = self
            .devices
            .devices
            .iter()
            .find(|d| d.device_id == device_id && d.app_id == app_id)
            .map(crate::edit::DeviceForm::new);
    }

    pub fn proxy_addr(&self) -> String {
        self.status
            .as_ref()
            .map(|s| s.listen.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "127.0.0.1:8899".into())
    }

    pub fn ca_path(&self) -> Option<String> {
        self.ca
            .as_ref()
            .map(|c| c.cert_path.clone())
            .filter(|p| !p.is_empty())
    }

    pub fn refresh_trust(&mut self, ctx: &Context) {
        let now = ctx.input(|i| i.time);
        if now - self.last_trust_check < 5.0 {
            return;
        }
        self.last_trust_check = now;
        if let Some(ca) = &self.ca {
            let pem = if ca.pem.is_empty() {
                std::fs::read_to_string(&ca.cert_path).unwrap_or_default()
            } else {
                ca.pem.clone()
            };
            self.trusted_system = Some(platform::installed_in_system(&platform::pem_body(&pem)));
            self.trusted_nss = platform::installed_in_nss();
        }
    }

    pub fn is_trusted(&self) -> Option<bool> {
        self.trusted_system
    }

    pub fn id(&self, s: &str) -> Id {
        Id::new(("proxy", s))
    }
}
