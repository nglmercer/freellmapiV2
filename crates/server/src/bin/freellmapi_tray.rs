//! Native tray-only desktop launcher.
//!
//! The launcher owns the local server process and opens the existing system
//! browser. It intentionally has no embedded webview, so the normal dashboard
//! remains the same HTTP application used by server deployments.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
#[cfg(target_os = "linux")]
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, SyncSender},
    Arc,
};
use std::thread;
#[cfg(target_os = "linux")]
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::WindowId;

const APP_DATA_DIR: &str = "FreeLLMAPI";
const ADMIN_KEY_FRAGMENT: &str = "adminKey";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(50);
const SHUTDOWN_CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
#[cfg(target_os = "linux")]
const TRAY_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_POLL_INTERVAL: Duration = Duration::from_secs(2);

const OPEN_MENU_ID: &str = "freellmapi.open-dashboard";
const SETUP_MENU_ID: &str = "freellmapi.open-setup";
const RESTART_MENU_ID: &str = "freellmapi.restart-server";
const QUIT_MENU_ID: &str = "freellmapi.quit";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrayAction {
    OpenDashboard,
    OpenSetup,
    RestartServer,
    Quit,
}

#[derive(Debug)]
enum UserEvent {
    Tray(TrayAction),
}

#[derive(Clone, Debug)]
struct TrayMenuIds {
    open: MenuId,
    setup: MenuId,
    restart: MenuId,
    quit: MenuId,
}

impl TrayMenuIds {
    fn action_for(&self, id: &MenuId) -> Option<TrayAction> {
        if id == &self.open {
            Some(TrayAction::OpenDashboard)
        } else if id == &self.setup {
            Some(TrayAction::OpenSetup)
        } else if id == &self.restart {
            Some(TrayAction::RestartServer)
        } else if id == &self.quit {
            Some(TrayAction::Quit)
        } else {
            None
        }
    }
}

struct TrayResources {
    _tray_icon: TrayIcon,
    ids: TrayMenuIds,
}

struct RunningServer {
    child: Child,
    port: u16,
    admin_key: String,
    stopped: bool,
}

impl RunningServer {
    fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;

        if self.child.try_wait().ok().flatten().is_none() {
            if self.request_graceful_shutdown() {
                self.wait_for_exit();
            }
            if self.child.try_wait().ok().flatten().is_none() {
                eprintln!("Server did not stop gracefully; forcing termination");
                let _ = self.child.kill();
            }
        }
        let _ = self.child.wait();
    }

    fn request_graceful_shutdown(&self) -> bool {
        #[cfg(unix)]
        if send_sigterm(self.child.id()) {
            return true;
        }

        self.request_http_shutdown()
    }

    fn request_http_shutdown(&self) -> bool {
        let address = SocketAddr::from(([127, 0, 0, 1], self.port));
        let Ok(mut stream) = TcpStream::connect_timeout(&address, SHUTDOWN_CONNECT_TIMEOUT) else {
            return false;
        };
        let _ = stream.set_read_timeout(Some(SHUTDOWN_CONNECT_TIMEOUT));
        let _ = stream.set_write_timeout(Some(SHUTDOWN_CONNECT_TIMEOUT));
        let request = format!(
            "POST /api/shutdown HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            self.port, self.admin_key
        );
        if stream.write_all(request.as_bytes()).is_err() {
            return false;
        }

        let mut response = [0_u8; 256];
        let Ok(read) = stream.read(&mut response) else {
            return false;
        };
        let response = std::str::from_utf8(&response[..read]).unwrap_or_default();
        response.starts_with("HTTP/1.1 202 ") || response.starts_with("HTTP/1.1 200 ")
    }

    fn wait_for_exit(&mut self) {
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => thread::sleep(SHUTDOWN_POLL_INTERVAL),
                Err(_) => return,
            }
        }
    }

    fn has_exited(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_some()
    }
}

#[cfg(unix)]
fn send_sigterm(pid: u32) -> bool {
    // The child PID comes directly from `Command::spawn`; SIGTERM lets the
    // server run its Tokio graceful-shutdown handler before we fall back to a
    // forceful kill.
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) == 0 }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Launcher {
    data_dir: PathBuf,
    static_dir: PathBuf,
    server_binary: PathBuf,
    preferred_admin_key: Option<String>,
    server: Option<RunningServer>,
}

impl Launcher {
    fn start(&mut self) -> Result<(), Box<dyn Error>> {
        if self.server.is_some() {
            return Ok(());
        }

        let port = free_port()?;
        let mut command = Command::new(&self.server_binary);
        command
            .current_dir(&self.data_dir)
            .env("BIND_ADDRESS", "127.0.0.1")
            .env("PORT", port.to_string())
            .env("DB_PATH", self.data_dir.join("freeapi.db"))
            .env("FREELLMAPI_CONFIG_DIR", &self.data_dir)
            .env("STATIC_DIR", &self.static_dir)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        if let Some(admin_key) = &self.preferred_admin_key {
            command.env("ADMIN_API_KEY", admin_key);
        } else {
            command.env_remove("ADMIN_API_KEY");
        }

        // The server's generated encryption key belongs to the tray app's
        // data directory. Do not accidentally inherit a different key from
        // the shell that launched the tray process.
        if let Some(encryption_key) = read_env_value(&self.data_dir.join(".env"), "ENCRYPTION_KEY")
            .filter(|key| valid_encryption_key(key))
        {
            command.env("ENCRYPTION_KEY", encryption_key);
        } else {
            command.env_remove("ENCRYPTION_KEY");
        }

        let mut child = command.spawn().map_err(|error| {
            format!(
                "Unable to start server binary {}: {error}",
                self.server_binary.display()
            )
        })?;
        let admin_key = match wait_for_server(
            &mut child,
            port,
            &self.data_dir,
            self.preferred_admin_key.as_deref(),
        ) {
            Ok(key) => key,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.into());
            }
        };
        self.preferred_admin_key = Some(admin_key.clone());
        self.server = Some(RunningServer {
            child,
            port,
            admin_key,
            stopped: false,
        });
        Ok(())
    }

    fn restart(&mut self) -> Result<(), Box<dyn Error>> {
        self.stop();
        self.start()
    }

    fn stop(&mut self) {
        if let Some(mut server) = self.server.take() {
            server.stop();
        }
    }

    fn dashboard_url(&self, path: &str) -> Option<String> {
        let server = self.server.as_ref()?;
        let path = if path.is_empty() {
            String::new()
        } else {
            format!("/{}", path.trim_start_matches('/'))
        };
        Some(format!(
            "http://127.0.0.1:{}{path}#{ADMIN_KEY_FRAGMENT}={}",
            server.port,
            encode_fragment(&server.admin_key)
        ))
    }

    fn open_dashboard(&self, path: &str) {
        let Some(url) = self.dashboard_url(path) else {
            eprintln!("FreeLLMAPI server is not running");
            return;
        };
        if let Err(error) = open::that(&url) {
            eprintln!("Unable to open dashboard in the default browser: {error}");
        }
    }

    fn report_if_server_exited(&mut self) {
        if self.server.as_mut().is_some_and(RunningServer::has_exited) {
            eprintln!("FreeLLMAPI server stopped");
            self.server = None;
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let data_dir = data_dir()?;
    fs::create_dir_all(&data_dir)?;
    restrict_directory_permissions(&data_dir)?;

    let event_loop = build_event_loop()?;
    let proxy = event_loop.create_proxy();

    #[cfg(target_os = "linux")]
    let mut linux_tray = Some(spawn_linux_tray(proxy.clone())?);

    #[cfg(target_os = "linux")]
    if env::var("FREELLMAPI_TRAY_SMOKE_TEST").as_deref() == Ok("1") {
        println!("Tray ready");
        if let Some(mut tray) = linux_tray.take() {
            tray.shutdown();
        }
        return Ok(());
    }

    let executable = env::current_exe()?;
    let static_dir = static_dir(&executable)?;
    let server_binary = server_binary(&executable)?;
    let env_path = data_dir.join(".env");
    let persisted_admin_key = read_env_value(&env_path, "ADMIN_API_KEY");
    let preferred_admin_key = persisted_admin_key.clone().or_else(|| {
        env::var("ADMIN_API_KEY")
            .ok()
            .filter(|key| server::env::is_valid_admin_api_key(key))
    });
    if persisted_admin_key.is_none() {
        if let Some(admin_key) = &preferred_admin_key {
            write_env_value(&env_path, "ADMIN_API_KEY", admin_key)?;
            restrict_file_permissions(&env_path)?;
        }
    }

    let mut launcher = Launcher {
        data_dir,
        static_dir,
        server_binary,
        preferred_admin_key,
        server: None,
    };
    launcher.start()?;

    #[cfg(target_os = "linux")]
    launcher.open_dashboard("");

    #[cfg(target_os = "linux")]
    let mut app = DesktopApp::new(launcher, linux_tray.take());
    #[cfg(not(target_os = "linux"))]
    let mut app = DesktopApp::new(launcher, proxy);

    let run_result = event_loop.run_app(&mut app);
    app.shutdown();
    run_result?;

    if let Some(error) = app.startup_error.take() {
        return Err(error.into());
    }

    Ok(())
}

fn build_event_loop() -> Result<EventLoop<UserEvent>, Box<dyn Error>> {
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    builder.build().map_err(|error| {
        #[cfg(target_os = "linux")]
        {
            let _ = error;
            linux_tray_startup_error().into()
        }
        #[cfg(not(target_os = "linux"))]
        {
            format!("Unable to initialize the desktop event loop: {error}").into()
        }
    })
}

fn create_tray_resources() -> Result<TrayResources, Box<dyn Error>> {
    let open_item = MenuItem::with_id(OPEN_MENU_ID, "Open dashboard", true, None);
    let setup_item = MenuItem::with_id(SETUP_MENU_ID, "Open setup", true, None);
    let restart_item = MenuItem::with_id(RESTART_MENU_ID, "Restart server", true, None);
    let quit_item = MenuItem::with_id(QUIT_MENU_ID, "Quit", true, None);
    let ids = TrayMenuIds {
        open: open_item.id().clone(),
        setup: setup_item.id().clone(),
        restart: restart_item.id().clone(),
        quit: quit_item.id().clone(),
    };
    let menu = Menu::with_items(&[&open_item, &setup_item, &restart_item, &quit_item])?;
    let tray_icon = TrayIconBuilder::new()
        .with_id("freellmapi")
        .with_menu(Box::new(menu))
        .with_tooltip("FreeLLMAPI")
        .with_icon(tray_icon_image()?)
        .build()?;

    Ok(TrayResources {
        _tray_icon: tray_icon,
        ids,
    })
}

fn install_menu_event_handler(proxy: EventLoopProxy<UserEvent>, ids: TrayMenuIds) {
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Some(action) = ids.action_for(&event.id) {
            let _ = proxy.send_event(UserEvent::Tray(action));
        }
    }));
}

#[cfg(target_os = "linux")]
enum GtkCommand {
    Quit,
}

#[cfg(target_os = "linux")]
struct LinuxTrayHandle {
    shutdown_tx: Option<gtk::glib::Sender<GtkCommand>>,
    thread: Option<JoinHandle<Result<(), String>>>,
}

#[cfg(target_os = "linux")]
impl LinuxTrayHandle {
    fn shutdown(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(GtkCommand::Quit);
        }

        if let Some(thread) = self.thread.take() {
            match thread.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("GTK tray thread stopped with an error: {error}"),
                Err(_) => eprintln!("GTK tray thread panicked during shutdown"),
            }
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for LinuxTrayHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(target_os = "linux")]
fn spawn_linux_tray(proxy: EventLoopProxy<UserEvent>) -> Result<LinuxTrayHandle, Box<dyn Error>> {
    let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), String>>(1);
    #[allow(deprecated)]
    let (shutdown_tx, shutdown_rx) =
        gtk::glib::MainContext::channel::<GtkCommand>(gtk::glib::Priority::default());
    let cancelled = Arc::new(AtomicBool::new(false));
    let thread_cancelled = Arc::clone(&cancelled);
    let thread_ready_tx = ready_tx.clone();
    let thread = thread::Builder::new()
        .name("freellmapi-gtk-tray".to_string())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_linux_tray(proxy, ready_tx, shutdown_rx, thread_cancelled)
            }))
            .unwrap_or_else(|_| Err(linux_tray_startup_error()));
            if let Err(error) = &result {
                let _ = thread_ready_tx.send(Err(error.clone()));
            }
            result
        })
        .map_err(|error| format!("Unable to start the GTK tray thread: {error}"))?;

    match ready_rx.recv_timeout(TRAY_STARTUP_TIMEOUT) {
        Ok(Ok(())) => Ok(LinuxTrayHandle {
            shutdown_tx: Some(shutdown_tx),
            thread: Some(thread),
        }),
        Ok(Err(error)) => {
            cancelled.store(true, Ordering::Release);
            let _ = shutdown_tx.send(GtkCommand::Quit);
            let _ = thread.join();
            Err(error.into())
        }
        Err(_) => {
            cancelled.store(true, Ordering::Release);
            let _ = shutdown_tx.send(GtkCommand::Quit);
            let _ = thread.join();
            Err(linux_tray_startup_error().into())
        }
    }
}

#[cfg(target_os = "linux")]
fn run_linux_tray(
    proxy: EventLoopProxy<UserEvent>,
    ready_tx: SyncSender<Result<(), String>>,
    shutdown_rx: gtk::glib::Receiver<GtkCommand>,
    cancelled: Arc<AtomicBool>,
) -> Result<(), String> {
    gtk::init().map_err(|_| linux_tray_startup_error())?;
    if cancelled.load(Ordering::Acquire) {
        return Ok(());
    }

    let resources = create_tray_resources().map_err(|_| linux_tray_startup_error())?;
    if cancelled.load(Ordering::Acquire) {
        return Ok(());
    }

    install_menu_event_handler(proxy, resources.ids.clone());

    let context = gtk::glib::MainContext::default();
    shutdown_rx.attach(Some(&context), |command| match command {
        GtkCommand::Quit => {
            gtk::main_quit();
            gtk::glib::ControlFlow::Break
        }
    });

    // Report readiness from an idle callback so the handshake proves that the
    // GTK main loop has started processing events, not only that the tray
    // objects were constructed.
    gtk::glib::idle_add_once(move || {
        let _ = ready_tx.send(Ok(()));
    });

    gtk::main();
    drop(resources);
    Ok(())
}

#[cfg(target_os = "linux")]
fn linux_tray_startup_error() -> String {
    "Unable to start desktop tray: no usable GTK graphical session was found, or GTK 3/AppIndicator support is unavailable. Start the launcher inside a graphical Linux session; for SSH, headless, or server deployments, run the server binary directly.".to_string()
}

struct DesktopApp {
    launcher: Launcher,
    startup_error: Option<String>,
    #[cfg(target_os = "linux")]
    linux_tray: Option<LinuxTrayHandle>,
    #[cfg(not(target_os = "linux"))]
    tray: Option<TrayResources>,
    #[cfg(not(target_os = "linux"))]
    proxy: EventLoopProxy<UserEvent>,
}

impl DesktopApp {
    #[cfg(target_os = "linux")]
    fn new(launcher: Launcher, linux_tray: Option<LinuxTrayHandle>) -> Self {
        Self {
            launcher,
            startup_error: None,
            linux_tray,
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn new(launcher: Launcher, proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            launcher,
            startup_error: None,
            tray: None,
            proxy,
        }
    }

    fn shutdown(&mut self) {
        self.launcher.stop();

        #[cfg(target_os = "linux")]
        if let Some(mut tray) = self.linux_tray.take() {
            tray.shutdown();
        }

        #[cfg(not(target_os = "linux"))]
        let _ = self.tray.take();
    }

    #[cfg(not(target_os = "linux"))]
    fn create_tray(&mut self, event_loop: &ActiveEventLoop) {
        if self.tray.is_some() {
            return;
        }

        match create_tray_resources() {
            Ok(resources) => {
                install_menu_event_handler(self.proxy.clone(), resources.ids.clone());
                self.tray = Some(resources);
                self.launcher.open_dashboard("");
            }
            Err(error) => {
                let message = format!("Unable to create desktop tray: {error}");
                eprintln!("{message}");
                self.startup_error = Some(message);
                event_loop.exit();
            }
        }
    }
}

impl Drop for DesktopApp {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl ApplicationHandler<UserEvent> for DesktopApp {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        #[cfg(not(target_os = "linux"))]
        if matches!(cause, StartCause::Init) {
            self.create_tray(event_loop);
        }

        #[cfg(target_os = "linux")]
        let _ = (event_loop, cause);
    }

    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Tray(TrayAction::OpenDashboard) => self.launcher.open_dashboard(""),
            UserEvent::Tray(TrayAction::OpenSetup) => self.launcher.open_dashboard("setup"),
            UserEvent::Tray(TrayAction::RestartServer) => {
                if let Err(error) = self.launcher.restart() {
                    eprintln!("Unable to restart FreeLLMAPI server: {error}");
                } else {
                    self.launcher.open_dashboard("");
                }
            }
            UserEvent::Tray(TrayAction::Quit) => event_loop.exit(),
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.launcher.report_if_server_exited();
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + SERVER_POLL_INTERVAL,
        ));
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.shutdown();
    }
}

fn data_dir() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = env::var_os("FREELLMAPI_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }
    dirs::data_local_dir()
        .map(|path| path.join(APP_DATA_DIR))
        .or_else(|| {
            env::current_dir()
                .ok()
                .map(|path| path.join("freellmapi-data"))
        })
        .ok_or_else(|| "Unable to determine a writable application-data directory".into())
}

fn static_dir(executable: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("FREELLMAPI_STATIC_DIR") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(parent) = executable.parent() {
        candidates.push(parent.join("client/dist"));
        candidates.push(parent.join("resources/client/dist"));
        candidates.push(parent.join("../resources/client/dist"));
    }
    candidates.push(server::env::project_root().join("client/dist"));

    candidates
        .into_iter()
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(path)
            }
        })
        .find(|path| path.join("index.html").is_file())
        .ok_or_else(|| "Unable to find client/dist/index.html; set FREELLMAPI_STATIC_DIR".into())
}

fn server_binary(executable: &Path) -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = env::var_os("FREELLMAPI_SERVER_BIN") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }

    let mut candidates = Vec::new();
    if let Some(parent) = executable.parent() {
        candidates.push(parent.join(server_executable_name()));
    }
    let root = server::env::project_root();
    candidates.push(root.join("target/release").join(server_executable_name()));
    candidates.push(root.join("target/debug").join(server_executable_name()));
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            "Unable to find the server binary; build it beside the tray binary or set FREELLMAPI_SERVER_BIN".into()
        })
}

fn server_executable_name() -> &'static str {
    if cfg!(windows) {
        "server.exe"
    } else {
        "server"
    }
}

fn free_port() -> io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

fn wait_for_server(
    child: &mut Child,
    port: u16,
    data_dir: &Path,
    preferred_admin_key: Option<&str>,
) -> Result<String, String> {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    let env_path = data_dir.join(".env");
    while Instant::now() < deadline {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Unable to monitor server startup: {error}"))?
        {
            return Err(format!("Server exited during startup with status {status}"));
        }
        if let Some(admin_key) = preferred_admin_key
            .filter(|key| server::env::is_valid_admin_api_key(key))
            .map(str::to_owned)
            .or_else(|| read_env_value(&env_path, "ADMIN_API_KEY"))
        {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                restrict_file_permissions(&env_path).map_err(|error| error.to_string())?;
                return Ok(admin_key);
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "Server did not become ready within {STARTUP_TIMEOUT:?}"
    ))
}

fn read_env_value(path: &Path, key: &str) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    content.lines().find_map(|line| {
        let line = line.trim();
        let value = line.strip_prefix(&format!("{key}="))?.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .or_else(|| {
                value
                    .strip_prefix('\'')
                    .and_then(|value| value.strip_suffix('\''))
            })
            .unwrap_or(value)
            .trim();
        if key == "ADMIN_API_KEY" && !server::env::is_valid_admin_api_key(value) {
            None
        } else {
            Some(value.to_string())
        }
    })
}

fn write_env_value(path: &Path, key: &str, value: &str) -> io::Result<()> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let prefix = format!("{key}=");
    let mut found = false;
    let mut updated = String::new();

    for line in content.lines() {
        if line.trim_start().starts_with(&prefix) {
            updated.push_str(&prefix);
            updated.push_str(value);
            found = true;
        } else {
            updated.push_str(line);
        }
        updated.push('\n');
    }

    if !found {
        updated.push_str(&prefix);
        updated.push_str(value);
        updated.push('\n');
    }
    fs::write(path, updated)
}

fn valid_encryption_key(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn encode_fragment(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                vec![byte as char]
            }
            byte => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn tray_icon_image() -> Result<Icon, Box<dyn Error>> {
    let size = 32_u32;
    let mut rgba = vec![0_u8; (size * size * 4) as usize];
    let center = (size as f32 - 1.0) / 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance <= 14.0 {
                let offset = ((y * size + x) * 4) as usize;
                rgba[offset] = 20;
                rgba[offset + 1] = 20;
                rgba[offset + 2] = 20;
                rgba[offset + 3] = 255;
            }
            if distance <= 5.0 {
                let offset = ((y * size + x) * 4) as usize;
                rgba[offset] = 255;
                rgba[offset + 1] = 255;
                rgba[offset + 2] = 255;
            }
        }
    }
    Ok(Icon::from_rgba(rgba, size, size)?)
}

fn restrict_directory_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn restrict_file_permissions(path: &Path) -> io::Result<()> {
    if !path.is_file() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu_ids() -> TrayMenuIds {
        TrayMenuIds {
            open: MenuId::new(OPEN_MENU_ID),
            setup: MenuId::new(SETUP_MENU_ID),
            restart: MenuId::new(RESTART_MENU_ID),
            quit: MenuId::new(QUIT_MENU_ID),
        }
    }

    #[test]
    fn menu_ids_map_to_explicit_tray_actions() {
        let ids = menu_ids();
        assert_eq!(ids.action_for(&ids.open), Some(TrayAction::OpenDashboard));
        assert_eq!(ids.action_for(&ids.setup), Some(TrayAction::OpenSetup));
        assert_eq!(
            ids.action_for(&ids.restart),
            Some(TrayAction::RestartServer)
        );
        assert_eq!(ids.action_for(&ids.quit), Some(TrayAction::Quit));
        assert_eq!(ids.action_for(&MenuId::new("freellmapi.unknown")), None);
    }

    #[cfg(unix)]
    #[test]
    fn running_server_stop_is_idempotent() {
        let child = Command::new("sh")
            .args(["-c", "exit 0"])
            .spawn()
            .expect("spawn test child");
        let mut server = RunningServer {
            child,
            port: 0,
            admin_key: "a".repeat(64),
            stopped: false,
        };

        server.stop();
        server.stop();

        assert!(server.stopped);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_tray_startup_error_explains_headless_fallback() {
        let error = linux_tray_startup_error();
        assert!(error.contains("GTK"));
        assert!(error.contains("graphical Linux session"));
        assert!(error.contains("server binary directly"));
    }
}
