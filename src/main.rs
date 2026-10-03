use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyEventKind,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::{stream, StreamExt};
use nexus_net::{
    app::{App, Effect, Modal},
    backend, command,
    config::{private_atomic, Config, History, Storage},
    control::{self, Plan},
    model::*,
    tools, ui,
};
use ratatui::{backend::CrosstermBackend, Terminal};
use serde_json::{json, Value};
use std::{
    io::{self, IsTerminal},
    path::PathBuf,
    time::Duration,
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};

#[derive(Parser)]
#[command(
    name = "nexus",
    version,
    about = "A terminal-native Linux Network Control Center"
)]
struct Cli {
    /// Print a local network snapshot as JSON (no external probes)
    #[arg(long)]
    snapshot: bool,
    /// List optional networking capabilities without opening the TUI
    #[arg(long)]
    doctor: bool,
    /// Write a local troubleshooting report to this path
    #[arg(long, value_name = "PATH")]
    report: Option<PathBuf>,
    /// Include private IPs, MACs, endpoint/process data in the report
    #[arg(long, requires = "report")]
    include_sensitive: bool,
    /// Write the actual live TUI's rendered cell buffer as JSON for preview/testing
    #[arg(long, value_name = "PATH")]
    render: Option<PathBuf>,
    #[arg(long, default_value_t = 140)]
    width: u16,
    #[arg(long, default_value_t = 42)]
    height: u16,
    /// Theme override for this session
    #[arg(long)]
    theme: Option<String>,
    /// Temporary session: ignore saved history and do not write settings/history
    #[arg(long)]
    fresh: bool,
    /// Graph renderer: auto-detect Kitty/compatible terminals, force Kitty, or portable text
    #[arg(long, value_parser = ["auto", "kitty", "text"])]
    chart_renderer: Option<String>,
}

enum Message {
    Snapshot(Result<Snapshot>),
    Refreshed(u64, Result<Snapshot>),
    Probes {
        values: Vec<(String, Option<f64>)>,
        dns: Option<f64>,
    },
    Finished(u64, Result<Box<ToolResult>>),
    Plan(u64, Result<Plan>),
    Wifi(u64, Vec<WifiNetwork>),
    Notice(String),
}
struct TerminalGuard(bool);
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.0 {
            let _ = nexus_net::graphics::cleanup(&mut io::stderr());
        }
        let _ = disable_raw_mode();
        let _ = execute!(io::stderr(), DisableMouseCapture, LeaveAlternateScreen);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let persist = !cli.fresh;
    let storage = Storage::new()?;
    let mut config = storage.load_config()?;
    if let Some(theme) = cli.theme {
        config.theme = theme;
    }
    if let Some(renderer) = cli.chart_renderer {
        config.chart_renderer = renderer;
    }
    let history = if cli.fresh {
        History::default()
    } else {
        storage.load_history()?
    };
    let mut app = App::new(config, history);
    let graphics_mode = nexus_net::graphics::Mode::detect(&app.config.chart_renderer);
    app.graphics.borrow_mut().mode = graphics_mode;
    if cli.render.is_some() && app.config.chart_renderer == "kitty" {
        app.graphics.borrow_mut().mode = nexus_net::graphics::Mode::Preview;
    }
    if cli.doctor {
        println!("NEXUS {} · Linux capabilities", env!("CARGO_PKG_VERSION"));
        for c in backend::linux::capabilities() {
            println!(
                "{:<16} {:<10} {}",
                c.command,
                if c.available { "available" } else { "missing" },
                c.purpose
            );
        }
        return Ok(());
    }
    if cli.snapshot || cli.report.is_some() || cli.render.is_some() {
        let snapshot = backend::system_backend()?.snapshot().await?;
        app.update_snapshot(snapshot);
        if cli.snapshot {
            println!("{}", serde_json::to_string_pretty(&app.snapshot)?);
        }
        if let Some(path) = cli.report {
            export_to(
                &path,
                &app,
                cli.include_sensitive,
                path.extension().is_some_and(|e| e == "txt"),
            )?;
            println!("Report written: {}", path.display());
        }
        if let Some(path) = cli.render {
            tokio::time::sleep(Duration::from_millis(500)).await;
            app.update_snapshot(backend::system_backend()?.snapshot().await?);
            render_buffer(&path, &app, cli.width, cli.height)?;
            println!("Rendered buffer: {}", path.display());
        }
        return Ok(());
    }
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        anyhow::bail!("Interactive mode needs a terminal. Use --snapshot, --doctor or --report PATH for headless use.");
    }
    // Restore the terminal before showing a panic or returning on any error.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if graphics_mode == nexus_net::graphics::Mode::Kitty {
            let _ = nexus_net::graphics::cleanup(&mut io::stderr());
        }
        let _ = disable_raw_mode();
        let _ = execute!(io::stderr(), DisableMouseCapture, LeaveAlternateScreen);
        previous(info);
    }));
    enable_raw_mode()?;
    let mut terminal_guard = TerminalGuard(graphics_mode == nexus_net::graphics::Mode::Kitty);
    execute!(io::stderr(), EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stderr()))?;
    terminal.clear()?;
    let (tx, mut rx) = mpsc::channel::<Message>(32);
    let (state_tx, state_rx) =
        watch::channel((app.config.clone(), app.snapshot.clone(), app.paused));
    let collector = collect(tx.clone(), state_rx.clone());
    let monitor = monitor(tx.clone(), state_rx);
    let mut input = EventStream::new();
    let mut ticks = tokio::time::interval(Duration::from_millis(33));
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut job: Option<JoinHandle<()>> = None;
    let mut task_id = 0u64;
    let mut save_tick = 0;
    loop {
        if let Ok(size) = crossterm::terminal::window_size() {
            app.graphics.borrow_mut().set_cell_size(
                size.width,
                size.height,
                size.columns,
                size.rows,
            );
        }
        terminal.draw(|f| ui::draw(f, &app))?;
        terminal_guard.0 |= app.graphics.borrow().mode == nexus_net::graphics::Mode::Kitty;
        app.graphics.borrow_mut().flush(&mut io::stderr())?;
        let mut effect = Effect::None;
        tokio::select! {
            _=ticks.tick()=>{app.tick+=1;save_tick+=1;if save_tick>=900{save_tick=0;effect=Effect::Save;}
                if app.tick%300==0 && !app.paused && app.busy.is_none() {
                    if app.page==nexus_net::app::Page::Pihole && app.pihole_polling { if let Some((url,password))=app.pihole_connection.clone(){effect=Effect::Run(tools::Tool::Pihole{url,password});} }
                    else if app.page==nexus_net::app::Page::Tailscale && app.tailscale_result.is_some(){effect=Effect::Run(tools::Tool::Tailscale);}
                }},
            _=tokio::signal::ctrl_c()=>{if !app.mutating{effect=Effect::Quit;}},
            item=input.next()=>match item {
                Some(Ok(Event::Key(key))) if key.kind!=KeyEventKind::Release=>{
                    if key.code==KeyCode::Enter&&matches!(&app.modal,Some(Modal::ConfirmPlan(_)))&&(terminal.size()?.width<80||terminal.size()?.height<24){app.notice("Resize to at least 80 × 24 to review and confirm a network change");continue;}
                    if key.code==KeyCode::Char('x') && app.modal.is_none()&&app.busy.is_some(){
                        if app.mutating{app.notice("Configuration operations finish or time out; cancellation could leave a partial change");}
                        else{task_id+=1;if let Some(h)=job.take(){h.abort();}app.busy=None;app.event("Task cancelled","Active diagnostic task cancelled".into());app.notice("Diagnostic task cancelled");}
                    }else{effect=app.handle_key(key);}
                },
                Some(Ok(Event::Mouse(mouse))) if app.modal.is_none()=>match mouse.kind{
                    MouseEventKind::ScrollDown=>{app.selected=(app.selected+1).min(app.rows().1.len().saturating_sub(1));},MouseEventKind::ScrollUp=>app.selected=app.selected.saturating_sub(1),
                    MouseEventKind::Down(crossterm::event::MouseButton::Left) if mouse.row==0=>{
                            if let Some(page)=ui::clicked_tab(mouse.column, terminal.size()?.width){app.navigate(page);}
                    },_=>{}
                },
                Some(Err(e))=>{app.notice(format!("Terminal input error: {e}"));effect=Effect::Quit;},None=>effect=Effect::Quit,_=>{}
            },
            Some(message)=rx.recv()=>match message{
                Message::Snapshot(Ok(s))=>{if !app.paused{app.update_snapshot(s);}},Message::Snapshot(Err(e))=>app.notice(format!("Snapshot: {e}")),
                Message::Refreshed(id,result)=>{if id==task_id{app.busy=None;match result{Ok(s)=>{app.update_snapshot(s);app.notice("Local network state refreshed");},Err(e)=>app.notice(format!("Refresh failed: {e}"))}}},
                Message::Probes{values,dns}=>app.update_probes(values,dns),Message::Wifi(id,networks)=>if id==task_id{app.wifi=networks;},
                Message::Plan(id,result)=>{if id==task_id{app.busy=None;app.mutating=false;match result{Ok(p)=>app.modal=Some(Modal::ConfirmPlan(p)),Err(e)=>app.notice(format!("Cannot prepare change: {e}"))}}},
                Message::Finished(id,result)=>{if id==task_id{let pihole_job=app.busy.as_deref()==Some("Pi-hole");app.busy=None;app.mutating=false;match result{Ok(r)=>app.finish_result(*r),Err(e)=>{if pihole_job{app.pihole_polling=false;}app.notice(format!("Task failed: {e}"));app.event("Task failed",e.to_string());}}effect=Effect::Save;}},
                Message::Notice(value)=>app.notice(value),
            }
        }
        match effect {
            Effect::Quit => break,
            Effect::None => {}
            Effect::Run(tool) => {
                if app.busy.is_some() {
                    app.notice("A task is already running");
                    continue;
                }
                task_id += 1;
                let id = task_id;
                if let tools::Tool::Pihole { url, password } = &tool {
                    app.pihole_connection = Some((url.clone(), password.clone()));
                    app.config.pihole_url = Some(url.clone());
                }
                app.busy = Some(tool.title().into());
                let s = app.snapshot.clone();
                let c = app.config.clone();
                let tx = tx.clone();
                job = Some(tokio::spawn(async move {
                    let wifi = matches!(tool, tools::Tool::Wifi { .. });
                    let result = tools::execute(tool, s, c).await;
                    if wifi && result.is_ok() {
                        if let Ok(networks) = tools::scan_wifi(false).await {
                            let _ = tx.send(Message::Wifi(id, networks)).await;
                        }
                    }
                    let _ = tx.send(Message::Finished(id, result.map(Box::new))).await;
                }));
            }
            Effect::Prepare(change) => {
                if app.busy.is_some() {
                    app.notice("Wait for the active task before preparing a configuration change");
                    continue;
                }
                task_id += 1;
                let id = task_id;
                app.busy = Some("Reading current configuration for preview".into());
                let tx = tx.clone();
                let s = app.snapshot.clone();
                job = Some(tokio::spawn(async move {
                    let _ = tx
                        .send(Message::Plan(id, control::prepare(change, &s).await))
                        .await;
                }));
            }
            Effect::Apply(plan) => {
                task_id += 1;
                let id = task_id;
                app.busy = Some(plan.title.clone());
                app.mutating = true;
                app.last_plan = Some(plan.clone());
                app.event("Change confirmed", plan.title.clone());
                let tx = tx.clone();
                job = Some(tokio::spawn(async move {
                    let _ = tx
                        .send(Message::Finished(
                            id,
                            control::apply(&plan).await.map(Box::new),
                        ))
                        .await;
                }));
            }
            Effect::Refresh => {
                if app.busy.is_none() {
                    task_id += 1;
                    let id = task_id;
                    app.busy = Some("Refreshing local state".into());
                    let tx = tx.clone();
                    job = Some(tokio::spawn(async move {
                        let r = match backend::system_backend() {
                            Ok(b) => b.snapshot().await,
                            Err(e) => Err(e),
                        };
                        let _ = tx.send(Message::Refreshed(id, r)).await;
                    }));
                }
            }
            Effect::Save => {
                if persist {
                    if let Err(e) = storage
                        .save_config(&app.config)
                        .and_then(|_| storage.save_history(&app.history))
                    {
                        app.notice(format!("Could not persist local settings: {e}"));
                    }
                }
            }
            Effect::Export { full, text } => {
                let path = storage.data_dir.join("exports").join(format!(
                    "nexus-report-{}.{}",
                    chrono::Utc::now().format("%Y%m%d-%H%M%S"),
                    if text { "txt" } else { "json" }
                ));
                match export_to(&path, &app, full, text) {
                    Ok(_) => app.notice(format!("Report: {}", path.display())),
                    Err(e) => app.notice(format!("Export failed: {e}")),
                }
            }
            Effect::Copy(value) => {
                let program = if command::available("wl-copy") {
                    Some(("wl-copy", vec![]))
                } else if command::available("xclip") {
                    Some(("xclip", vec!["-selection".into(), "clipboard".into()]))
                } else {
                    None
                };
                if let Some((p, a)) = program {
                    let tx = tx.clone();
                    let value = value.into_bytes();
                    tokio::spawn(async move {
                        let r = command::run_with_input(p, &a, Some(value), Duration::from_secs(2))
                            .await;
                        let _ = tx
                            .send(Message::Notice(match r {
                                Ok(_) => "Copied selected row".into(),
                                Err(e) => format!("Copy failed: {e}"),
                            }))
                            .await;
                    });
                } else {
                    app.notice("No clipboard backend. Install wl-copy (Wayland) or xclip (X11).");
                }
            }
        }
        let _ = state_tx.send((app.config.clone(), app.snapshot.clone(), app.paused));
    }
    if let Some(job) = job {
        job.abort();
    }
    collector.abort();
    monitor.abort();
    if persist {
        storage.save_config(&app.config)?;
        storage.save_history(&app.history)?;
    }
    terminal.show_cursor()?;
    Ok(())
}

fn collect(
    tx: mpsc::Sender<Message>,
    state: watch::Receiver<(Config, Snapshot, bool)>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let backend = match backend::system_backend() {
            Ok(b) => b,
            Err(e) => {
                let _ = tx.send(Message::Snapshot(Err(e))).await;
                return;
            }
        };
        loop {
            let (config, _, paused) = state.borrow().clone();
            if !paused {
                let result = backend.snapshot().await;
                if tx.send(Message::Snapshot(result)).await.is_err() {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_secs(config.refresh_seconds)).await;
        }
    })
}
fn monitor(
    tx: mpsc::Sender<Message>,
    state: watch::Receiver<(Config, Snapshot, bool)>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let (config, snapshot, paused) = state.borrow().clone();
            if config.monitoring_enabled && !paused {
                let mut targets = Vec::new();
                if let Some(gw) = snapshot.gateway().filter(|s| *s != "—") {
                    targets.push(gw.to_string());
                }
                if config.external_enabled {
                    targets.extend(config.targets.clone());
                }
                targets.sort();
                targets.dedup();
                let values = stream::iter(targets)
                    .map(|host| async move {
                        let value = tools::ping_once(&host).await;
                        (host, value)
                    })
                    .buffer_unordered(8)
                    .collect()
                    .await;
                let dns = if config.external_enabled {
                    tools::dns_lookup(&config.dns_test_name, "A", "")
                        .await
                        .ok()
                        .and_then(|d| d.metrics.get("Query time (ms)")?.parse().ok())
                } else {
                    None
                };
                if tx.send(Message::Probes { values, dns }).await.is_err() {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    })
}

fn report(app: &App, sensitive: bool) -> Result<Value> {
    let mut snapshot = serde_json::to_value(&app.snapshot)?;
    let mut tests = serde_json::to_value(&app.history.tests)?;
    let mut events = serde_json::to_value(&app.history.events)?;
    let mut diagnostic = serde_json::to_value(&app.result)?;
    let mut diagnostic_history = serde_json::to_value(&app.history.tool_results)?;
    let mut services = json!({"tailscale":app.tailscale_result,"pihole":app.pihole_result});
    if !sensitive {
        redact(&mut snapshot);
        redact(&mut tests);
        redact(&mut events);
        redact(&mut diagnostic);
        redact(&mut diagnostic_history);
        redact(&mut services);
    }
    Ok(
        json!({"application":"NEXUS","version":env!("CARGO_PKG_VERSION"),"generated_at":chrono::Utc::now(),"redacted":!sensitive,"external_access_enabled":app.config.external_enabled,"snapshot":snapshot,"samples":app.history.samples,"speed_tests":tests,"recent_events":events,"latest_tool_result":diagnostic,"service_observations":services,"diagnostic_history":diagnostic_history,"notes":["No Wi-Fi credentials, WireGuard private keys or proxy passwords are read/exported.","Listener wildcard binding is not proof of public exposure.","Internet status is unmeasured until explicit tests or enabled monitoring."]}),
    )
}
fn redact(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if [
                    "addresses",
                    "ips",
                    "endpoint",
                    "path",
                    "self_name",
                    "exit_node",
                    "dns_suffix",
                    "health",
                    "destination",
                    "ssid",
                    "bssid",
                    "mac",
                    "local",
                    "remote",
                    "executable",
                    "process",
                    "pid",
                    "uid",
                    "inode",
                    "primary",
                    "dns",
                    "gateway",
                    "interface",
                    "ip",
                    "connection",
                    "proxies",
                    "name",
                    "detail",
                    "rows",
                    "notes",
                    "warnings",
                    "evidence",
                    "next_step",
                ]
                .contains(&key.as_str())
                {
                    *value = json!("[redacted]");
                } else if key == "metrics" {
                    if let Some(metrics) = value.as_object_mut() {
                        for (k, v) in metrics {
                            if !k.contains("Mbps") && k != "Outcome" {
                                *v = json!("[redacted]");
                            }
                        }
                    }
                } else {
                    redact(value);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                redact(item);
            }
        }
        _ => {}
    }
}
fn export_to(path: &std::path::Path, app: &App, sensitive: bool, text: bool) -> Result<()> {
    let value = report(app, sensitive)?;
    let data = if text {
        format!(
            "NEXUS NETWORK TROUBLESHOOTING REPORT\nGenerated: {}\nRedacted: {}\n\n{}\n",
            chrono::Utc::now(),
            !sensitive,
            serde_json::to_string_pretty(&value)?
        )
    } else {
        serde_json::to_string_pretty(&value)?
    };
    private_atomic(path, data.as_bytes())
}
fn render_buffer(path: &std::path::Path, app: &App, width: u16, height: u16) -> Result<()> {
    let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(
        width.clamp(20, 240),
        height.clamp(10, 80),
    ))?;
    terminal.draw(|f| ui::draw(f, app))?;
    let buffer = terminal.backend().buffer();
    let mut cells = Vec::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let c = &buffer[(x, y)];
            cells.push(json!({"x":x,"y":y,"symbol":c.symbol(),"fg":format!("{:?}",c.fg),"bg":format!("{:?}",c.bg),"bold":c.modifier.contains(ratatui::style::Modifier::BOLD)}));
        }
    }
    let graphics = app.graphics.borrow().preview_assets(path)?;
    private_atomic(
        path,
        &serde_json::to_vec(
            &json!({"width":buffer.area.width,"height":buffer.area.height,"cells":cells,"graphics":graphics}),
        )?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reports_redact_endpoint_and_process_metadata() {
        let mut v = json!({"connections":[{"local":"127.0.0.1:22","process":"secret","pid":22}],"mac":"aa:bb","metrics":{"Download Mbps":"100","Backend":"private-server"}});
        redact(&mut v);
        assert_eq!(v["connections"][0]["local"], "[redacted]");
        assert_eq!(v["metrics"]["Download Mbps"], "100");
        assert_eq!(v["metrics"]["Backend"], "[redacted]");
    }
}
