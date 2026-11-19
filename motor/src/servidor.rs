//! Servidor HTTP mínimo en 127.0.0.1: un hilo por conexión, una petición por conexión.
//! Solo atiende a la propia máquina y comprueba la cabecera Host y la ficha `t`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

pub struct Resp {
    pub estado: u16,
    pub tipo: &'static str,
    pub cuerpo: Vec<u8>,
}

impl Resp {
    pub fn json(x: String) -> Resp { Resp { estado: 200, tipo: "application/json; charset=utf-8", cuerpo: x.into_bytes() } }
    pub fn html(x: &[u8]) -> Resp { Resp { estado: 200, tipo: "text/html; charset=utf-8", cuerpo: x.to_vec() } }
    pub fn bin(x: Vec<u8>) -> Resp { Resp { estado: 200, tipo: "application/octet-stream", cuerpo: x } }
    pub fn error(estado: u16, x: &str) -> Resp { Resp { estado, tipo: "text/plain; charset=utf-8", cuerpo: x.as_bytes().to_vec() } }
}

pub struct Pet {
    pub ruta: String,
    pub q: HashMap<String, String>,
}

pub type Manejador = dyn Fn(&Pet) -> Resp + Send + Sync;

pub fn abre() -> std::io::Result<(TcpListener, u16)> {
    let l = TcpListener::bind(("127.0.0.1", 0))?;
    let p = l.local_addr()?.port();
    Ok((l, p))
}

pub fn sirve(l: TcpListener, puerto: u16, m: Arc<Manejador>) {
    for c in l.incoming() {
        let Ok(c) = c else { continue };
        let m = m.clone();
        let _ = std::thread::Builder::new().stack_size(1 << 20).spawn(move || { let _ = atiende(c, puerto, &*m); });
    }
}

fn atiende(c: TcpStream, puerto: u16, m: &Manejador) -> std::io::Result<()> {
    c.set_read_timeout(Some(std::time::Duration::from_secs(20)))?;
    let mut r = BufReader::new(c.try_clone()?);
    let mut linea = String::new();
    r.read_line(&mut linea)?;
    let mut partes = linea.split_whitespace();
    let (_metodo, destino) = (partes.next().unwrap_or(""), partes.next().unwrap_or("/"));
    let (mut host_ok, mut largo) = (false, 0usize);
    loop {
        let mut h = String::new();
        if r.read_line(&mut h)? == 0 { break; }
        let h = h.trim_end();
        if h.is_empty() { break; }
        if let Some((k, v)) = h.split_once(':') {
            let v = v.trim();
            match k.to_ascii_lowercase().as_str() {
                "host" => host_ok = v == format!("127.0.0.1:{puerto}") || v == format!("localhost:{puerto}"),
                "content-length" => largo = v.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    if largo > 0 && largo < 1 << 20 { let mut b = vec![0u8; largo]; let _ = r.read_exact(&mut b); }
    let (ruta, q) = destino.split_once('?').unwrap_or((destino, ""));
    let resp = if !host_ok { Resp::error(403, "host no permitido") } else { m(&Pet { ruta: ruta.to_string(), q: crate::json::consulta(q) }) };
    let mut c = c;
    let txt = match resp.estado { 200 => "OK", 403 => "Forbidden", 404 => "Not Found", _ => "Error" };
    let cab = format!("HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n", resp.estado, txt, resp.tipo, resp.cuerpo.len());
    c.write_all(cab.as_bytes())?;
    c.write_all(&resp.cuerpo)?;
    c.flush()
}
