//! Descompresor Snappy "framed" propio (sin dependencias).
//! Chunks: 0xff stream-id · 0x00 comprimido (CRC4 + bloque) · 0x01 sin comprimir (CRC4 + datos) · 0x80..0xfe saltables.

fn uvarint(b: &[u8]) -> Result<(usize, usize), String> {
    let (mut v, mut s, mut i) = (0usize, 0u32, 0usize);
    loop {
        let x = *b.get(i).ok_or("varint truncado")?;
        i += 1;
        v |= ((x & 0x7f) as usize) << s;
        if x & 0x80 == 0 {
            return Ok((v, i));
        }
        s += 7;
    }
}

fn bloque(src: &[u8], dst: &mut Vec<u8>) -> Result<(), String> {
    let (ulen, n) = uvarint(src)?;
    let fin = dst.len() + ulen;
    dst.reserve(ulen);
    let mut s = &src[n..];
    let err = || "bloque snappy corrupto".to_string();
    while !s.is_empty() {
        let tag = s[0];
        let (off, l);
        match tag & 3 {
            0 => {
                let mut ln = (tag >> 2) as usize;
                s = &s[1..];
                if ln >= 60 {
                    let nb = ln - 59;
                    if s.len() < nb { return Err(err()); }
                    ln = 0;
                    for i in 0..nb { ln |= (s[i] as usize) << (8 * i); }
                    s = &s[nb..];
                }
                ln += 1;
                if s.len() < ln { return Err(err()); }
                dst.extend_from_slice(&s[..ln]);
                s = &s[ln..];
                continue;
            }
            1 => {
                if s.len() < 2 { return Err(err()); }
                l = 4 + ((tag >> 2) & 7) as usize;
                off = (((tag >> 5) as usize) << 8) | s[1] as usize;
                s = &s[2..];
            }
            2 => {
                if s.len() < 3 { return Err(err()); }
                l = 1 + (tag >> 2) as usize;
                off = u16::from_le_bytes([s[1], s[2]]) as usize;
                s = &s[3..];
            }
            _ => {
                if s.len() < 5 { return Err(err()); }
                l = 1 + (tag >> 2) as usize;
                off = u32::from_le_bytes([s[1], s[2], s[3], s[4]]) as usize;
                s = &s[5..];
            }
        }
        if off == 0 || off > dst.len() { return Err(err()); }
        let st = dst.len() - off;
        for i in 0..l {
            let b = dst[st + i];
            dst.push(b);
        }
    }
    if dst.len() != fin { return Err(err()); }
    Ok(())
}

/// Descomprime un fichero Snappy framed completo (el .duf entero).
pub fn framed(raw: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(raw.len() * 2);
    let mut o = 0usize;
    while o + 4 <= raw.len() {
        let typ = raw[o];
        let ln = raw[o + 1] as usize | (raw[o + 2] as usize) << 8 | (raw[o + 3] as usize) << 16;
        let body = o + 4;
        if body + ln > raw.len() { return Err("chunk snappy truncado".into()); }
        match typ {
            0xff => {}
            0x00 => bloque(&raw[body + 4..body + ln], &mut out)?,
            0x01 => out.extend_from_slice(&raw[body + 4..body + ln]),
            t if t >= 0x80 => {}
            t => return Err(format!("chunk snappy no soportado: {t:#x}")),
        }
        o = body + ln;
    }
    Ok(out)
}

/* ------------------------------ compresión (para restaurar ficheros archivados) ------------------------------ */

fn crc32c(b: &[u8]) -> u32 {
    static TABLA: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let t = TABLA.get_or_init(|| {
        let mut t = [0u32; 256];
        for i in 0..256u32 {
            let mut c = i;
            for _ in 0..8 { c = if c & 1 != 0 { (c >> 1) ^ 0x82F63B78 } else { c >> 1 }; }
            t[i as usize] = c;
        }
        t
    });
    let mut c = !0u32;
    for &x in b { c = t[((c ^ x as u32) & 0xff) as usize] ^ (c >> 8); }
    !c
}

fn crc_enmascarado(b: &[u8]) -> u32 { let c = crc32c(b); ((c >> 15) | (c << 17)).wrapping_add(0xa282ead8) }

fn literal(src: &[u8], dst: &mut Vec<u8>) {
    if src.is_empty() { return; }
    let n = src.len() - 1;
    if n < 60 { dst.push((n as u8) << 2); }
    else if n < 256 { dst.push(60 << 2); dst.push(n as u8); }
    else { dst.push(61 << 2); dst.extend_from_slice(&(n as u16).to_le_bytes()); }
    dst.extend_from_slice(src);
}

fn copia(mut largo: usize, off: usize, dst: &mut Vec<u8>) {
    // copias de 2 bytes de desplazamiento (off < 65536, como mucho 64 bytes cada una)
    while largo > 0 {
        let l = if largo > 64 { if largo - 60 < 4 { 60 } else { 64 } } else { largo };
        let l = l.min(64);
        if l < 4 { break; }
        dst.push((((l - 1) as u8) << 2) | 2);
        dst.extend_from_slice(&(off as u16).to_le_bytes());
        largo -= l;
    }
}

/// Comprime un bloque de hasta 65 536 bytes (formato Snappy: varint con el tamaño y elementos).
fn comprime_bloque(src: &[u8], dst: &mut Vec<u8>) {
    let mut n = src.len();
    loop { let b = (n & 0x7f) as u8; n >>= 7; if n == 0 { dst.push(b); break; } dst.push(b | 0x80); }
    if src.len() < 16 { literal(src, dst); return; }
    let mut tabla = [0u16; 1 << 13];
    let h = |v: u32| (v.wrapping_mul(0x1e35a7bd) >> 19) as usize;
    let u32en = |i: usize| u32::from_le_bytes([src[i], src[i + 1], src[i + 2], src[i + 3]]);
    let (mut i, mut ini) = (1usize, 0usize);
    let fin = src.len() - 4;
    while i <= fin {
        let v = u32en(i);
        let k = h(v);
        let cand = tabla[k] as usize;
        tabla[k] = i as u16;
        if cand < i && u32en(cand) == v {
            let mut l = 4;
            while i + l < src.len() && src[cand + l] == src[i + l] { l += 1; }
            literal(&src[ini..i], dst);
            let antes = dst.len();
            copia(l, i - cand, dst);
            // si quedó un resto de menos de 4 bytes sin copiar, se deja para el literal siguiente
            let copiado: usize = { let mut c = 0; let mut j = antes; while j < dst.len() { c += ((dst[j] >> 2) as usize) + 1; j += 3; } c };
            i += copiado;
            ini = i;
        } else {
            i += 1;
        }
    }
    literal(&src[ini..], dst);
}

/// Comprime en Snappy framed (trozos de 64 KiB; si no gana nada, el trozo va sin comprimir).
pub fn enmarca(plano: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(b"\xff\x06\x00\x00sNaPpY");
    let mut tmp = Vec::with_capacity(80_000);
    for t in plano.chunks(65536) {
        tmp.clear();
        comprime_bloque(t, &mut tmp);
        let (tipo, cuerpo): (u8, &[u8]) = if tmp.len() < t.len() - t.len() / 8 { (0, &tmp) } else { (1, t) };
        let l = cuerpo.len() + 4;
        out.extend_from_slice(&[tipo, l as u8, (l >> 8) as u8, (l >> 16) as u8]);
        out.extend_from_slice(&crc_enmascarado(t).to_le_bytes());
        out.extend_from_slice(cuerpo);
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    #[test]
    fn ida_y_vuelta() {
        let mut x = 12345u64;
        let mut datos = Vec::new();
        for i in 0..400_000u32 {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            if i % 7 < 4 { datos.extend_from_slice(b"SCHEDULER\\TASK SOLIDS "); } else { datos.push((x >> 33) as u8); }
            if i % 1000 == 0 { datos.extend_from_slice(&[0u8; 300]); }
        }
        for d in [&datos[..], &datos[..70_000], &datos[..15], &[][..], &vec![7u8; 200_000][..]] {
            let mut f = Vec::new();
            enmarca(d, &mut f);
            assert_eq!(framed(&f).unwrap(), d);
            if d.len() > 100_000 { assert!(f.len() < d.len()); }
        }
        // valor conocido de CRC-32C
        assert_eq!(crc32c(b"123456789"), 0xE3069283);
    }
}
