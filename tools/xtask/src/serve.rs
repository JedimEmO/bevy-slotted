//! `cargo xtask serve`: a static file server for `dist/`.
//!
//! Small on purpose. `python3 -m http.server` works too, but it guesses
//! `application/octet-stream` for `.wasm` on some installs, and
//! `WebAssembly.instantiateStreaming` refuses anything but
//! `application/wasm`. One correct table beats a documented workaround.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};

use crate::wasm::workspace_root;

/// `cargo xtask serve [--dir <path>] [--port <n>]`.
pub fn serve(args: &[String]) -> Result<(), String> {
    let mut dir = workspace_root().join("dist");
    let mut port: u16 = 8080;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--dir" => {
                let value = iter.next().ok_or("--dir needs a path")?;
                dir = PathBuf::from(value);
            }
            "--port" => {
                let value = iter.next().ok_or("--port needs a number")?;
                port = value.parse().map_err(|_| format!("bad port `{value}`"))?;
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    let dir = dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}; run `cargo xtask playground` first", dir.display()))?;

    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("binding 127.0.0.1:{port}: {e}"))?;
    eprintln!(
        "xtask: serving {} on http://127.0.0.1:{port}/",
        dir.display()
    );
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle(stream, &dir) {
                    eprintln!("xtask: {error}");
                }
            }
            Err(error) => eprintln!("xtask: accept: {error}"),
        }
    }
    Ok(())
}

/// One request, one response, one connection. No keep-alive: a browser opens
/// a handful of connections for a page this size and the loop is never the
/// bottleneck.
fn handle(mut stream: TcpStream, root: &Path) -> Result<(), String> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("reading the request: {e}"))?;
    // Drain the headers so the client does not see a reset before the body.
    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) => break,
            Ok(_) if header.trim().is_empty() => break,
            Ok(_) => {}
            Err(e) => return Err(format!("reading headers: {e}")),
        }
    }

    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/");
    if method != "GET" && method != "HEAD" {
        return respond(&mut stream, 405, "text/plain", b"method not allowed");
    }

    let path = target.split(['?', '#']).next().unwrap_or("/");
    let Some(file) = resolve(root, path) else {
        return respond(&mut stream, 404, "text/plain", b"not found");
    };
    let mut bytes = Vec::new();
    match std::fs::File::open(&file) {
        Ok(mut handle) => {
            handle
                .read_to_end(&mut bytes)
                .map_err(|e| format!("{}: {e}", file.display()))?;
        }
        Err(_) => return respond(&mut stream, 404, "text/plain", b"not found"),
    }
    let mime = mime_of(&file);
    if method == "HEAD" {
        bytes.clear();
    }
    respond(&mut stream, 200, mime, &bytes)
}

/// The file a request path names, refusing anything that climbs out of `root`.
fn resolve(root: &Path, path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(path);
    let mut out = root.to_path_buf();
    for segment in decoded.split('/').filter(|s| !s.is_empty() && *s != ".") {
        if segment == ".." {
            return None;
        }
        out.push(segment);
    }
    if out.components().any(|c| matches!(c, Component::ParentDir)) {
        return None;
    }
    if !out.starts_with(root) {
        return None;
    }
    if out.is_dir() {
        out.push("index.html");
    }
    out.is_file().then_some(out)
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The content type, with `.wasm` spelled correctly.
fn mime_of(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("css") => "text/css; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("ron" | "lua" | "toml" | "ftl" | "wgsl" | "txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn respond(stream: &mut TcpStream, status: u16, mime: &str, body: &[u8]) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "Error",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {mime}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body))
        .and_then(|()| stream.flush())
        .map_err(|e| format!("writing the response: {e}"))
}
