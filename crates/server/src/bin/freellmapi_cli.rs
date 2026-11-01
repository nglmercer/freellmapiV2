//! `freellmapi-cli`: a terminal client for a running FreeLLMAPI server.
//!
//! Connection settings come from flags or environment variables:
//! - `FREELLMAPI_URL` (default `http://127.0.0.1:3001`)
//! - `FREELLMAPI_API_KEY`: an API key from the dashboard, or
//! - `FREELLMAPI_USER` plus `FREELLMAPI_PASSWORD`: signs in as a dashboard user.
//!
//! The password is read only from the environment, never from a flag, so it
//! does not end up in shell history. Nothing is written to disk.

use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use base64::Engine;
use clap::{Parser, Subcommand};
use serde_json::{json, Value};

const DEFAULT_URL: &str = "http://127.0.0.1:3001";
/// Largest file the CLI will read into memory (the server limits uploads too).
const MAX_FILE_BYTES: u64 = 15 * 1024 * 1024;
const MAX_TEXT_BYTES: u64 = 1024 * 1024;

#[derive(Parser, Debug)]
#[command(
    name = "freellmapi-cli",
    version,
    about = "Chat with and test models on a running FreeLLMAPI server"
)]
struct Cli {
    /// Server base URL.
    #[arg(long, global = true, env = "FREELLMAPI_URL", default_value = DEFAULT_URL)]
    url: String,

    /// API key from the dashboard.
    #[arg(
        long,
        global = true,
        env = "FREELLMAPI_API_KEY",
        hide_env_values = true
    )]
    key: Option<String>,

    /// Dashboard username (used with FREELLMAPI_PASSWORD).
    #[arg(long, global = true, env = "FREELLMAPI_USER")]
    user: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Chat with a model. Without a prompt, starts an interactive session.
    Chat(ChatArgs),
    /// List models, optionally only those declared for an operation.
    Models {
        /// For example: speech, transcription, image_generation, embeddings.
        #[arg(long)]
        capability: Option<String>,
    },
    /// Text to speech. Writes the audio to a file.
    Speak {
        text: String,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(short, long)]
        model: String,
        #[arg(short, long, default_value = "alloy")]
        voice: String,
        /// Audio format, for example mp3 or wav. Defaults to the output extension.
        #[arg(short, long)]
        format: Option<String>,
    },
    /// Speech to text.
    Transcribe {
        file: PathBuf,
        #[arg(short, long)]
        model: String,
        /// Translate the speech to English instead.
        #[arg(long)]
        translate: bool,
    },
    /// Generate an image and save it to a file.
    Image {
        prompt: String,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(short, long)]
        model: String,
    },
    /// Embed one or more texts and print a summary.
    Embed {
        #[arg(required = true)]
        texts: Vec<String>,
        #[arg(short, long)]
        model: String,
    },
}

#[derive(clap::Args, Debug, Default)]
struct ChatArgs {
    /// The message. Omit it to read from stdin, or to open an interactive session.
    prompt: Vec<String>,
    /// Model id. `auto` lets the router choose.
    #[arg(short, long, default_value = "auto")]
    model: String,
    /// Attach an image (repeatable).
    #[arg(short = 'i', long = "image")]
    images: Vec<PathBuf>,
    /// Attach an audio file (repeatable).
    #[arg(short = 'a', long = "audio")]
    audio: Vec<PathBuf>,
    /// Attach a text document (repeatable).
    #[arg(short = 'd', long = "doc")]
    docs: Vec<PathBuf>,
    /// System instruction.
    #[arg(long)]
    system: Option<String>,
}

/// How requests authenticate.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Auth {
    None,
    Bearer(String),
    Cookie(String),
}

struct Client {
    http: reqwest::Client,
    base: String,
    auth: Auth,
}

impl Client {
    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let builder = self
            .http
            .request(method, format!("{}{path}", self.base))
            .timeout(Duration::from_secs(300));
        match &self.auth {
            Auth::None => builder,
            Auth::Bearer(key) => builder.bearer_auth(key),
            Auth::Cookie(cookie) => builder.header("Cookie", cookie),
        }
    }

    /// Returns the response, or the server's error message for non-2xx codes.
    async fn send(&self, builder: reqwest::RequestBuilder) -> Result<reqwest::Response, String> {
        let res = builder
            .send()
            .await
            .map_err(|e| format!("cannot reach the server at {}: {e}", self.base))?;
        if res.status().is_success() {
            return Ok(res);
        }
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        Err(format!("HTTP {status}: {}", error_message(&text)))
    }

    async fn json(&self, builder: reqwest::RequestBuilder) -> Result<Value, String> {
        let res = self.send(builder).await?;
        res.json::<Value>()
            .await
            .map_err(|e| format!("invalid JSON from server: {e}"))
    }
}

/// Pulls `error.message` out of an OpenAI-style error body, if present.
fn error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v["error"]["message"]
                .as_str()
                .or_else(|| v["error"].as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.trim().to_string())
}

/// Signs in with a username and password and returns a `Cookie` header value.
async fn sign_in(
    http: &reqwest::Client,
    base: &str,
    user: &str,
    password: &str,
) -> Result<String, String> {
    let res = http
        .post(format!("{base}/api/auth/sign-in/username"))
        .json(&json!({ "username": user, "password": password }))
        .send()
        .await
        .map_err(|e| format!("cannot reach the server at {base}: {e}"))?;
    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(format!(
            "sign-in failed (HTTP {status}): {}",
            error_message(&text)
        ));
    }
    // Keep only `name=value` pairs; attributes such as Path or Expires are dropped.
    let cookies: Vec<String> = res
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(|v| v.split(';').next())
        .map(|pair| pair.trim().to_string())
        .filter(|pair| pair.contains('='))
        .collect();
    if cookies.is_empty() {
        return Err("sign-in succeeded but the server returned no session cookie".to_string());
    }
    Ok(cookies.join("; "))
}

/// MIME type for an image path, by extension.
fn image_mime(path: &Path) -> Result<&'static str, String> {
    match extension(path).as_str() {
        "png" => Ok("image/png"),
        "jpg" | "jpeg" => Ok("image/jpeg"),
        "webp" => Ok("image/webp"),
        "gif" => Ok("image/gif"),
        other => Err(format!(
            "{}: unsupported image type '.{other}' (use png, jpg, webp or gif)",
            path.display()
        )),
    }
}

/// Container format for an audio path, by extension.
fn audio_format(path: &Path) -> Result<&'static str, String> {
    match extension(path).as_str() {
        "wav" => Ok("wav"),
        "mp3" => Ok("mp3"),
        "flac" => Ok("flac"),
        "ogg" => Ok("ogg"),
        "opus" => Ok("opus"),
        "m4a" => Ok("m4a"),
        "aac" => Ok("aac"),
        "webm" => Ok("webm"),
        other => Err(format!(
            "{}: unsupported audio type '.{other}' (use wav, mp3, flac, ogg, m4a, webm)",
            path.display()
        )),
    }
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn read_limited(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > max {
        return Err(format!(
            "{}: {} bytes is larger than the {} MB limit",
            path.display(),
            meta.len(),
            max / (1024 * 1024)
        ));
    }
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// One user message: text plus any attached images, audio and documents.
fn user_message(
    text: &str,
    images: &[PathBuf],
    audio: &[PathBuf],
    docs: &[PathBuf],
) -> Result<Value, String> {
    let mut text_full = text.to_string();
    for doc in docs {
        let body = String::from_utf8(read_limited(doc, MAX_TEXT_BYTES)?).map_err(|_| {
            format!(
                "{}: not a UTF-8 text file (PDF and Word are not supported)",
                doc.display()
            )
        })?;
        text_full.push_str(&format!("\n\n--- {} ---\n{body}", doc.display()));
    }
    if images.is_empty() && audio.is_empty() {
        return Ok(json!({ "role": "user", "content": text_full }));
    }
    let mut parts: Vec<Value> = Vec::new();
    if !text_full.is_empty() {
        parts.push(json!({ "type": "text", "text": text_full }));
    }
    for image in images {
        let mime = image_mime(image)?;
        let data = read_limited(image, MAX_FILE_BYTES)?;
        parts.push(json!({
            "type": "image_url",
            "image_url": { "url": format!("data:{mime};base64,{}", b64(&data)) }
        }));
    }
    for clip in audio {
        let format = audio_format(clip)?;
        let data = read_limited(clip, MAX_FILE_BYTES)?;
        parts.push(json!({
            "type": "input_audio",
            "input_audio": { "data": b64(&data), "format": format }
        }));
    }
    Ok(json!({ "role": "user", "content": parts }))
}

/// The assistant's text from a chat completion response.
fn answer_text(body: &Value) -> String {
    match &body["choices"][0]["message"]["content"] {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
        _ => "(the model returned no text)".to_string(),
    }
}

/// Sends the conversation and returns the answer and the serving route.
async fn send_chat(
    client: &Client,
    model: &str,
    messages: &[Value],
) -> Result<(String, Option<String>), String> {
    let mut body = json!({ "messages": messages });
    if model != "auto" {
        body["model"] = json!(model);
    }
    let res = client
        .send(
            client
                .request(reqwest::Method::POST, "/v1/chat/completions")
                .json(&body),
        )
        .await?;
    let routed = res
        .headers()
        .get("x-routed-via")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let value: Value = res
        .json()
        .await
        .map_err(|e| format!("invalid JSON from server: {e}"))?;
    Ok((answer_text(&value), routed))
}

async fn cmd_chat(client: &Client, args: ChatArgs) -> Result<(), String> {
    let mut messages: Vec<Value> = Vec::new();
    if let Some(system) = &args.system {
        messages.push(json!({ "role": "system", "content": system }));
    }

    let stdin_is_pipe = !io::stdin().is_terminal();
    let prompt = args.prompt.join(" ");
    if !prompt.is_empty()
        || !args.images.is_empty()
        || !args.audio.is_empty()
        || !args.docs.is_empty()
    {
        let text = if prompt.is_empty() && stdin_is_pipe {
            read_stdin()?
        } else {
            prompt
        };
        messages.push(user_message(&text, &args.images, &args.audio, &args.docs)?);
        let (answer, routed) = send_chat(client, &args.model, &messages).await?;
        print_answer(&answer, routed.as_deref());
        return Ok(());
    }
    if stdin_is_pipe {
        let text = read_stdin()?;
        if text.trim().is_empty() {
            return Err("no prompt given (pass text as arguments or pipe it on stdin)".to_string());
        }
        messages.push(user_message(&text, &[], &[], &[])?);
        let (answer, routed) = send_chat(client, &args.model, &messages).await?;
        print_answer(&answer, routed.as_deref());
        return Ok(());
    }
    interactive(client, args.model, messages).await
}

fn read_stdin() -> Result<String, String> {
    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| format!("cannot read stdin: {e}"))?;
    Ok(buf.trim_end().to_string())
}

fn print_answer(answer: &str, routed: Option<&str>) {
    println!("{answer}");
    if let Some(route) = routed {
        eprintln!("[served by {route}]");
    }
}

/// Interactive session. Attachments added with `/image`, `/audio` or `/doc`
/// go with the next message.
async fn interactive(
    client: &Client,
    mut model: String,
    mut messages: Vec<Value>,
) -> Result<(), String> {
    println!("FreeLLMAPI chat. Model: {model}. Type /help for commands, /exit to quit.");
    let stdin = io::stdin();
    let mut pending_images: Vec<PathBuf> = Vec::new();
    let mut pending_audio: Vec<PathBuf> = Vec::new();
    let mut pending_docs: Vec<PathBuf> = Vec::new();
    loop {
        print!("you> ");
        io::stdout().flush().ok();
        let mut line = String::new();
        if stdin.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            println!();
            return Ok(());
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(cmd) = line.strip_prefix('/') {
            let (name, arg) = cmd
                .split_once(' ')
                .map(|(a, b)| (a, b.trim()))
                .unwrap_or((cmd, ""));
            match name {
                "exit" | "quit" => return Ok(()),
                "help" => println!(
                    "/image PATH  attach an image\n/audio PATH  attach audio\n/doc PATH    attach a text file\n/model NAME  switch model\n/clear       start over\n/exit        quit"
                ),
                "model" if !arg.is_empty() => {
                    model = arg.to_string();
                    println!("model: {model}");
                }
                "image" if !arg.is_empty() => pending_images.push(PathBuf::from(arg)),
                "audio" if !arg.is_empty() => pending_audio.push(PathBuf::from(arg)),
                "doc" if !arg.is_empty() => pending_docs.push(PathBuf::from(arg)),
                "clear" => {
                    messages.retain(|m| m["role"] == "system");
                    println!("conversation cleared");
                }
                _ => println!("unknown command or missing argument: /{name}"),
            }
            continue;
        }
        let message = match user_message(line, &pending_images, &pending_audio, &pending_docs) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("error: {e}");
                continue;
            }
        };
        pending_images.clear();
        pending_audio.clear();
        pending_docs.clear();
        messages.push(message);
        match send_chat(client, &model, &messages).await {
            Ok((answer, routed)) => {
                println!("assistant> {answer}");
                if let Some(route) = routed {
                    eprintln!("[served by {route}]");
                }
                messages.push(json!({ "role": "assistant", "content": answer }));
            }
            Err(e) => {
                // Drop the failed user turn so the history stays consistent.
                messages.pop();
                eprintln!("error: {e}");
            }
        }
    }
}

async fn cmd_models(client: &Client, capability: Option<String>) -> Result<(), String> {
    let path = match &capability {
        Some(c) => format!("/v1/models?capability={c}"),
        None => "/v1/models".to_string(),
    };
    let body = client
        .json(client.request(reqwest::Method::GET, &path))
        .await?;
    let data = body["data"].as_array().cloned().unwrap_or_default();
    if data.is_empty() {
        println!("no models found");
        return Ok(());
    }
    for m in data {
        let caps = m["capabilities"]
            .as_array()
            .map(|c| {
                c.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        println!(
            "{:<48} {:<16} {}",
            m["id"].as_str().unwrap_or("?"),
            m["owned_by"].as_str().unwrap_or("?"),
            caps
        );
    }
    Ok(())
}

async fn cmd_speak(
    client: &Client,
    text: String,
    output: PathBuf,
    model: String,
    voice: String,
    format: Option<String>,
) -> Result<(), String> {
    let format = format.unwrap_or_else(|| {
        output
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp3")
            .to_string()
    });
    let body = json!({ "model": model, "input": text, "voice": voice, "response_format": format });
    let res = client
        .send(
            client
                .request(reqwest::Method::POST, "/v1/audio/speech")
                .json(&body),
        )
        .await?;
    let bytes = res.bytes().await.map_err(|e| e.to_string())?;
    std::fs::write(&output, &bytes).map_err(|e| format!("{}: {e}", output.display()))?;
    println!("wrote {} bytes to {}", bytes.len(), output.display());
    Ok(())
}

async fn cmd_transcribe(
    client: &Client,
    file: PathBuf,
    model: String,
    translate: bool,
) -> Result<(), String> {
    let data = read_limited(&file, MAX_FILE_BYTES)?;
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("audio")
        .to_string();
    let part = reqwest::multipart::Part::bytes(data)
        .file_name(name)
        .mime_str("application/octet-stream")
        .map_err(|e| e.to_string())?;
    let form = reqwest::multipart::Form::new()
        .text("model", model)
        .part("file", part);
    let path = if translate {
        "/v1/audio/translations"
    } else {
        "/v1/audio/transcriptions"
    };
    let res = client
        .send(client.request(reqwest::Method::POST, path).multipart(form))
        .await?;
    let body = res.text().await.map_err(|e| e.to_string())?;
    match serde_json::from_str::<Value>(&body) {
        Ok(v) => println!("{}", v["text"].as_str().unwrap_or(&body)),
        Err(_) => println!("{body}"),
    }
    Ok(())
}

async fn cmd_image(
    client: &Client,
    prompt: String,
    output: PathBuf,
    model: String,
) -> Result<(), String> {
    let body = json!({ "model": model, "prompt": prompt });
    let value = client
        .json(
            client
                .request(reqwest::Method::POST, "/v1/images/generations")
                .json(&body),
        )
        .await?;
    let first = &value["data"][0];
    if let Some(b64_data) = first["b64_json"].as_str() {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64_data)
            .map_err(|e| format!("invalid image data from server: {e}"))?;
        std::fs::write(&output, &bytes).map_err(|e| format!("{}: {e}", output.display()))?;
        println!("wrote {} bytes to {}", bytes.len(), output.display());
    } else if let Some(url) = first["url"].as_str() {
        println!("the provider returned a URL instead of data: {url}");
    } else {
        return Err("the server returned no image".to_string());
    }
    Ok(())
}

async fn cmd_embed(client: &Client, texts: Vec<String>, model: String) -> Result<(), String> {
    let body = json!({ "model": model, "input": texts });
    let value = client
        .json(
            client
                .request(reqwest::Method::POST, "/v1/embeddings")
                .json(&body),
        )
        .await?;
    let data = value["data"].as_array().cloned().unwrap_or_default();
    let dim = data
        .first()
        .and_then(|d| d["embedding"].as_array())
        .map(|v| v.len())
        .unwrap_or(0);
    println!("{} vectors, dimension {dim}", data.len());
    if let Some(first) = data.first().and_then(|d| d["embedding"].as_array()) {
        let preview: Vec<String> = first
            .iter()
            .take(8)
            .map(|n| format!("{:.4}", n.as_f64().unwrap_or(0.0)))
            .collect();
        println!("[{}, ...]", preview.join(", "));
    }
    Ok(())
}

async fn run(cli: Cli) -> Result<(), String> {
    let base = cli.url.trim_end_matches('/').to_string();
    let http = reqwest::Client::builder()
        .user_agent("freellmapi-cli/0.1")
        .build()
        .map_err(|e| e.to_string())?;

    let auth = if let Some(key) = cli.key.filter(|k| !k.is_empty()) {
        Auth::Bearer(key)
    } else if let Some(user) = cli.user.as_deref() {
        let password = std::env::var("FREELLMAPI_PASSWORD")
            .ok()
            .filter(|p| !p.is_empty())
            .ok_or_else(|| "FREELLMAPI_PASSWORD is not set".to_string())?;
        Auth::Cookie(sign_in(&http, &base, user, &password).await?)
    } else {
        Auth::None
    };
    let client = Client { http, base, auth };

    match cli.command.unwrap_or(Command::Chat(ChatArgs::default())) {
        Command::Chat(args) => cmd_chat(&client, args).await,
        Command::Models { capability } => cmd_models(&client, capability).await,
        Command::Speak {
            text,
            output,
            model,
            voice,
            format,
        } => cmd_speak(&client, text, output, model, voice, format).await,
        Command::Transcribe {
            file,
            model,
            translate,
        } => cmd_transcribe(&client, file, model, translate).await,
        Command::Image {
            prompt,
            output,
            model,
        } => cmd_image(&client, prompt, output, model).await,
        Command::Embed { texts, model } => cmd_embed(&client, texts, model).await,
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_media_types() {
        assert_eq!(image_mime(Path::new("cat.PNG")).unwrap(), "image/png");
        assert_eq!(image_mime(Path::new("a.jpeg")).unwrap(), "image/jpeg");
        assert!(image_mime(Path::new("a.bmp")).is_err());
        assert_eq!(audio_format(Path::new("clip.mp3")).unwrap(), "mp3");
        assert!(audio_format(Path::new("clip.txt")).is_err());
    }

    #[test]
    fn plain_text_message_stays_a_string() {
        let m = user_message("hola", &[], &[], &[]).unwrap();
        assert_eq!(m["content"], "hola");
    }

    #[test]
    fn image_message_becomes_content_parts_with_data_url() {
        let dir = std::env::temp_dir().join("freellmapi-cli-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pixel.png");
        std::fs::write(&path, [0x89, b'P', b'N', b'G']).unwrap();
        let m = user_message("what is this?", std::slice::from_ref(&path), &[], &[]).unwrap();
        let parts = m["content"].as_array().unwrap();
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[1]["type"], "image_url");
        assert!(parts[1]["image_url"]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));
    }

    #[test]
    fn audio_message_carries_base64_and_format() {
        let dir = std::env::temp_dir().join("freellmapi-cli-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clip.wav");
        std::fs::write(&path, b"RIFF").unwrap();
        let m = user_message("listen", &[], std::slice::from_ref(&path), &[]).unwrap();
        let part = &m["content"][1];
        assert_eq!(part["type"], "input_audio");
        assert_eq!(part["input_audio"]["format"], "wav");
        assert_eq!(part["input_audio"]["data"], b64(b"RIFF"));
    }

    #[test]
    fn document_text_is_added_to_the_message() {
        let dir = std::env::temp_dir().join("freellmapi-cli-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notas.txt");
        std::fs::write(&path, "contenido").unwrap();
        let m = user_message("resume", &[], &[], std::slice::from_ref(&path)).unwrap();
        let text = m["content"].as_str().unwrap();
        assert!(text.starts_with("resume"));
        assert!(text.contains("contenido"));
    }

    #[test]
    fn reads_answer_text_from_string_or_parts() {
        let s = json!({"choices": [{"message": {"content": "hi"}}]});
        assert_eq!(answer_text(&s), "hi");
        let parts = json!({"choices": [{"message": {"content": [{"type": "text", "text": "a"}, {"type": "text", "text": "b"}]}}]});
        assert_eq!(answer_text(&parts), "ab");
        assert_eq!(answer_text(&json!({})), "(the model returned no text)");
    }

    #[test]
    fn extracts_error_message_from_openai_body() {
        assert_eq!(
            error_message(r#"{"error":{"message":"bad key"}}"#),
            "bad key"
        );
        assert_eq!(error_message("plain text"), "plain text");
    }

    #[test]
    fn password_is_not_a_flag() {
        // Only the environment carries the password; the CLI has no such flag.
        assert!(Cli::try_parse_from(["freellmapi-cli", "--password", "x", "models"]).is_err());
    }

    #[test]
    fn subcommands_parse() {
        let cli =
            Cli::try_parse_from(["freellmapi-cli", "chat", "hola", "-m", "x/y", "-i", "a.png"])
                .unwrap();
        match cli.command {
            Some(Command::Chat(args)) => {
                assert_eq!(args.prompt, vec!["hola"]);
                assert_eq!(args.model, "x/y");
                assert_eq!(args.images.len(), 1);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(Cli::try_parse_from([
            "freellmapi-cli",
            "speak",
            "hi",
            "-o",
            "out.mp3",
            "-m",
            "tts"
        ])
        .is_ok());
    }
}
