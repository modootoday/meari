use meari::{
    domain::build::{BuildPath, TargetKind},
    infrastructure::artifact_store::FsArtifactStore,
};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    time::{Duration, Instant},
};

fn request(stream: &mut TcpStream, root: &Path) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !bytes.ends_with(b"\r\n\r\n") {
        if Instant::now() >= deadline {
            return Err("HTTP header deadline exceeded".into());
        }
        let mut byte = [0u8; 1];
        if stream.read(&mut byte).map_err(|e| e.to_string())? == 0 {
            return Err("incomplete HTTP request".into());
        }
        bytes.push(byte[0]);
        if bytes.len() > 8192 {
            return Err("HTTP headers exceed budget".into());
        }
    }
    let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    let mut parts = text
        .lines()
        .next()
        .ok_or("missing HTTP request")?
        .split_whitespace();
    if parts.next() != Some("GET") {
        return Err("only GET is supported".into());
    }
    let url = parts
        .next()
        .ok_or("missing request path")?
        .split('?')
        .next()
        .ok_or("missing path")?;
    // Generated bundle URLs are ASCII paths; encoded paths are unnecessary and rejected.
    if !url.starts_with('/') || url.contains('%') {
        return Err("invalid bundle URL".into());
    }
    let relative = if url == "/" { "index.html" } else { &url[1..] };
    let relative = BuildPath::try_from(relative.to_owned())?;
    let path = root.join(relative.as_str());
    let mut segment = root.to_path_buf();
    for part in relative.as_str().split('/') {
        segment.push(part);
        if super::files::is_link(&std::fs::symlink_metadata(&segment).map_err(|e| e.to_string())?) {
            return Err("bundle links are forbidden".into());
        }
    }
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(root) || !path.is_file() {
        return Err("bundle file unavailable".into());
    }
    let data = std::fs::read(&path).map_err(|e| e.to_string())?;
    let mime = match path.extension().and_then(|s| s.to_str()) {
        Some("wasm") => "application/wasm",
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("html") => "text/html; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    };
    write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",data.len()).map_err(|e|e.to_string())?;
    stream.write_all(&data).map_err(|e| e.to_string())
}

pub fn serve(store: &Path, port: u16, seconds: u64) -> Result<(), String> {
    if port == 0 || seconds == 0 || seconds > 86400 {
        return Err("explicit port and duration (1..86400 seconds) required".into());
    }
    let publication = FsArtifactStore::read_publication(store)?.ok_or("no published build")?;
    let artifact = publication
        .artifacts
        .iter()
        .find(|a| a.target.kind == TargetKind::Web)
        .ok_or("no Web build")?;
    let root = store
        .join("objects")
        .join(artifact.identity.as_str())
        .join("files/web")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let listener =
        TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    println!(
        "http://127.0.0.1:{port}/ generation={}",
        publication.generation
    );
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if request(&mut stream, &root).is_err() {
                    let _ = stream.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn exchange(root: &Path, bytes: &[u8]) -> (String, Result<(), String>) {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let root = root.to_path_buf();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            request(&mut stream, &root)
        });
        let mut client = TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        client.write_all(bytes).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();
        let mut output = String::new();
        client.read_to_string(&mut output).unwrap();
        (output, worker.join().unwrap())
    }
    #[test]
    fn bundle_http_serves_wasm_mime_and_rejects_escape_or_incomplete_headers() {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).unwrap();
        let root = std::env::temp_dir().join(format!(
            "meari-http-{}",
            meari::domain::build::ContentHash::of(&random).as_str()
        ));
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        std::fs::write(root.join("app.wasm"), b"wasm").unwrap();
        let (response, result) = exchange(
            &root,
            b"GET /app.wasm?v=1 HTTP/1.1\r\nHost: localhost\r\n\r\n",
        );
        result.unwrap();
        assert!(response.contains("Content-Type: application/wasm\r\n"));
        assert!(response.ends_with("wasm"));
        for (file, mime) in [
            ("logo.svg", "image/svg+xml"),
            ("font.ttf", "font/ttf"),
            ("font.woff2", "font/woff2"),
            ("image.webp", "image/webp"),
        ] {
            std::fs::write(root.join(file), b"asset-bytes").unwrap();
            let request = format!("GET /{file} HTTP/1.1\r\nHost: localhost\r\n\r\n");
            let (response, result) = exchange(&root, request.as_bytes());
            result.unwrap();
            assert!(response.contains(&format!("Content-Type: {mime}\r\n")));
            assert!(response.ends_with("asset-bytes"));
        }
        for request in [
            b"GET /../secret HTTP/1.1\r\n\r\n".as_slice(),
            b"GET /%2e%2e/secret HTTP/1.1\r\n\r\n",
            b"POST /app.wasm HTTP/1.1\r\n\r\n",
            b"GET /app.wasm HTTP/1.1\r\n",
        ] {
            let (response, result) = exchange(&root, request);
            assert!(result.is_err());
            assert!(response.is_empty());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
