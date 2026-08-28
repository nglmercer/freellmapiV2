//! Native tray-only desktop launcher.
//!
//! The launcher owns the local server process and opens the existing system
//! browser. It intentionally has no embedded webview, so the normal dashboard
//! remains the same HTTP application used by server deployments.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIconBuilder};
use winit::event::Event;
use winit::event_loop::{ControlFlow, EventLoop};

const APP_DATA_DIR: &str = "FreeLLMAPI";
const ADMIN_KEY_FRAGMENT: &str = "adminKey";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);

enum UserEvent {
    Menu(MenuEvent),
}

struct RunningServer {
    child: Child,
    port: u16,
    admin_key: String,
}

impl RunningServer {
    fn stop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }

    fn has_exited(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_some()
    }
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
        });
        Ok(())
    }

    fn restart(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(mut server) = self.server.take() {
            server.stop();
        }
        self.start()
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

    initialize_tray_backend()?;

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
    launcher.open_dashboard("");

    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Menu(event));
    }));

    let open_item = MenuItem::new("Open dashboard", true, None);
    let setup_item = MenuItem::new("Open setup", true, None);
    let restart_item = MenuItem::new("Restart server", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    let open_id = open_item.id().clone();
    let setup_id = setup_item.id().clone();
    let restart_id = restart_item.id().clone();
    let quit_id = quit_item.id().clone();
    let menu = Menu::with_items(&[&open_item, &setup_item, &restart_item, &quit_item])?;
    let tray_icon = TrayIconBuilder::new()
        .with_id("freellmapi")
        .with_menu(Box::new(menu))
        .with_tooltip("FreeLLMAPI")
        .with_icon(tray_icon_image()?)
        .build()?;

    #[allow(deprecated)]
    let run_result = event_loop.run(move |event, event_loop| {
        event_loop.set_control_flow(ControlFlow::wait_duration(Duration::from_secs(2)));
        match event {
            Event::UserEvent(UserEvent::Menu(event)) if event.id == open_id => {
                launcher.open_dashboard("");
            }
            Event::UserEvent(UserEvent::Menu(event)) if event.id == setup_id => {
                launcher.open_dashboard("setup");
            }
            Event::UserEvent(UserEvent::Menu(event)) if event.id == restart_id => {
                if let Err(error) = launcher.restart() {
                    eprintln!("Unable to restart FreeLLMAPI server: {error}");
                } else {
                    launcher.open_dashboard("");
                }
            }
            Event::UserEvent(UserEvent::Menu(event)) if event.id == quit_id => {
                event_loop.exit();
            }
            Event::AboutToWait => launcher.report_if_server_exited(),
            _ => {}
        }
        let _ = &tray_icon;
    });
    run_result?;

    Ok(())
}

fn initialize_tray_backend() -> Result<(), Box<dyn Error>> {
    #[cfg(target_os = "linux")]
    gtk::init().map_err(|error| {
        format!(
            "Unable to initialize the Linux tray backend. Start this app inside a graphical session with GTK available: {error}"
        )
    })?;

    Ok(())
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
