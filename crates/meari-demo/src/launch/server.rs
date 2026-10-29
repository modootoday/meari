//! Minimal static file server on 127.0.0.1 for the web pane (runs on a thread of the controller,
//! so it ends with the demo; no extra process to clean up).
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};

pub struct Server {
    pub port: u16,
}

impl Server {
    pub fn start(root: &Path) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("bind: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let root = root.to_path_buf();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = respond(&root, stream);
            }
        });
        Ok(Self { port })
    }

    pub fn url(&self, page: &str) -> String {
        format!("http://127.0.0.1:{}/{page}", self.port)
    }
}

fn respond(root: &Path, mut stream: TcpStream) -> std::io::Result<()> {
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let target = line.split_whitespace().nth(1).unwrap_or("/");
    let (status, kind, body) =
        match resolve(root, target).and_then(|p| std::fs::read(&p).ok().map(|b| (p, b))) {
            Some((path, body)) => ("200 OK", content_type(&path), body),
            None => ("404 Not Found", "text/plain", b"not found".to_vec()),
        };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)
}

/// Map a request path into `root`, refusing anything that climbs out of it.
fn resolve(root: &Path, target: &str) -> Option<PathBuf> {
    let relative = Path::new(target.split('?').next()?.trim_start_matches('/'));
    relative
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
        .then(|| root.join(relative))
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::resolve;
    use std::path::Path;

    #[test]
    fn refuses_parent_traversal() {
        assert!(resolve(Path::new("/srv"), "/../etc/passwd").is_none());
        assert_eq!(
            resolve(Path::new("/srv"), "/a/web.html?x=1").unwrap(),
            Path::new("/srv/a/web.html")
        );
    }
}
