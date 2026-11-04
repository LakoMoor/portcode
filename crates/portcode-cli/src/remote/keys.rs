//! Pairing keys for remote control. A key is 32 random bytes; the secret never crosses the network:
//! the server sends a nonce and the client proves it knows the secret with HMAC-SHA256(secret, nonce).
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub id: String,
    /// `view` (watch only) or `full` (send prompts, answer permission asks, switch agent/model/mode).
    pub scope: String,
    pub secret: String,
    pub created: u64,
}

pub fn random_hex(bytes: usize) -> Result<String, String> {
    let mut b = vec![0u8; bytes];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .map_err(|e| format!("cannot read /dev/urandom: {e}"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let (mut ipad, mut opad) = ([0x36u8; 64], [0x5cu8; 64]);
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let inner = Sha256::new()
        .chain_update(ipad)
        .chain_update(msg)
        .finalize();
    Sha256::new()
        .chain_update(opad)
        .chain_update(inner)
        .finalize()
        .into()
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Compare without leaking where the first difference is.
pub fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

pub fn mac_for(secret: &str, nonce: &str) -> String {
    hex(&hmac_sha256(secret.as_bytes(), nonce.as_bytes()))
}

fn path() -> Result<PathBuf, String> {
    portcode_agent::config::global_dir()
        .map(|d| d.join("remote-keys.json"))
        .ok_or_else(|| "no config directory".to_string())
}

pub fn load() -> Vec<Key> {
    let Ok(p) = path() else { return vec![] };
    let Ok(t) = std::fs::read_to_string(p) else {
        return vec![];
    };
    let Ok(v) = serde_json::from_str::<Vec<serde_json::Value>>(&t) else {
        return vec![];
    };
    v.iter()
        .filter_map(|k| {
            Some(Key {
                id: k.get("id")?.as_str()?.into(),
                scope: k.get("scope")?.as_str()?.into(),
                secret: k.get("secret")?.as_str()?.into(),
                created: k.get("created")?.as_u64().unwrap_or(0),
            })
        })
        .collect()
}

fn save(keys: &[Key]) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let p = path()?;
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let v: Vec<_> = keys
        .iter()
        .map(|k| serde_json::json!({"id":k.id,"scope":k.scope,"secret":k.secret,"created":k.created}))
        .collect();
    let tmp = p.with_extension("tmp");
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        f.write_all(
            serde_json::to_string_pretty(&v)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
    }
    std::fs::rename(tmp, p).map_err(|e| e.to_string())
}

pub fn create(scope: &str) -> Result<Key, String> {
    if scope != "view" && scope != "full" {
        return Err("scope must be 'view' or 'full'".into());
    }
    let k = Key {
        id: random_hex(4)?,
        scope: scope.into(),
        secret: random_hex(32)?,
        created: portcode_evidence::now_secs(),
    };
    let mut all = load();
    all.push(k.clone());
    save(&all)?;
    Ok(k)
}

pub fn revoke(id: &str) -> Result<bool, String> {
    let mut all = load();
    let n = all.len();
    all.retain(|k| k.id != id);
    save(&all)?;
    Ok(all.len() != n)
}

/// The string the user pastes into the web page: `pc1.<id>.<secret>`.
pub fn pairing(k: &Key) -> String {
    format!("pc1.{}.{}", k.id, k.secret)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hmac_matches_rfc4231_case_2() {
        // key "Jefe", data "what do ya want for nothing?"
        assert_eq!(
            hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }
    #[test]
    fn long_keys_and_ct_eq() {
        assert_eq!(hmac_sha256(&[7u8; 100], b"x").len(), 32);
        assert!(ct_eq("abc", "abc") && !ct_eq("abc", "abd") && !ct_eq("abc", "ab"));
    }
}
