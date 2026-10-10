use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use reqwest::{Client, Response};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    time::Duration,
};
use url::Url;

pub fn client() -> Result<Client> {
    Ok(Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .user_agent("StudHubLauncher/0.4.0")
        .build()?)
}
pub fn origin(raw: &str) -> Result<String> {
    let u = Url::parse(raw)?;
    if u.scheme() != "https"
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.fragment().is_some()
    {
        bail!("Разрешены только HTTPS-адреса без пароля и фрагмента")
    }
    Ok(u.origin().ascii_serialization())
}
pub async fn trusted(raw: &str, origins: &[String]) -> Result<()> {
    let u = Url::parse(raw)?;
    let o = origin(raw)?;
    if !origins.iter().any(|v| origin(v).ok().as_ref() == Some(&o)) {
        bail!("Источник загрузки не входит в доверенный список: {o}")
    }
    let host = u.host_str().unwrap();
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::BTreeMap<String, std::time::Instant>>,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if cache
        .lock()
        .unwrap()
        .get(host)
        .map_or(false, |t| t.elapsed() < Duration::from_secs(60))
    {
        return Ok(());
    }
    let addresses = match tokio::net::lookup_host((host, u.port_or_known_default().unwrap())).await
    {
        Ok(a) => a.map(|v| v.ip()).collect::<Vec<_>>(),
        Err(e) => {
            // HTTPS proxies can provide origin DNS while the local network has no DNS.
            // Validate the origin using authenticated DNS-over-HTTPS instead of accepting an unresolved host.
            if std::env::var("HTTPS_PROXY").is_err() && std::env::var("https_proxy").is_err() {
                return Err(e.into());
            }
            let c = client()?;
            let mut addresses = Vec::new();
            for record in ["A", "AAAA"] {
                let response = checked(
                    c.get("https://dns.google/resolve")
                        .query(&[("name", host), ("type", record)])
                        .send()
                        .await?,
                )
                .await?;
                let v: Value = response.json().await?;
                if let Some(answer) = v["Answer"].as_array() {
                    addresses.extend(
                        answer
                            .iter()
                            .filter(|a| a["type"] == 1 || a["type"] == 28)
                            .filter_map(|a| {
                                a["data"]
                                    .as_str()
                                    .and_then(|v| v.parse::<std::net::IpAddr>().ok())
                            }),
                    );
                }
            }
            if addresses.is_empty() {
                bail!("Не удалось подтвердить публичный адрес источника")
            }
            addresses
        }
    };
    if addresses.is_empty() {
        bail!("Источник не имеет публичных адресов")
    }
    for ip in addresses {
        let bad = match ip {
            std::net::IpAddr::V4(a) => {
                a.is_private()
                    || a.is_loopback()
                    || a.is_link_local()
                    || a.is_unspecified()
                    || a.is_multicast()
                    || a.is_broadcast()
                    || a.octets()[0] == 0
            }
            std::net::IpAddr::V6(a) => {
                a.is_loopback()
                    || a.is_unspecified()
                    || a.is_multicast()
                    || (a.segments()[0] & 0xfe00 == 0xfc00)
                    || (a.segments()[0] & 0xffc0 == 0xfe80)
                    || a.to_ipv4_mapped().is_some()
            }
        };
        if bad {
            bail!("Частный или локальный источник загрузки запрещён")
        }
    }
    cache
        .lock()
        .unwrap()
        .insert(host.to_owned(), std::time::Instant::now());
    Ok(())
}
pub async fn checked(r: Response) -> Result<Response> {
    if r.status().is_redirection() {
        bail!("Перенаправление источника запрещено")
    }
    if !r.status().is_success() {
        bail!("Сервер вернул HTTP {}", r.status())
    }
    Ok(r)
}
pub async fn json(c: &Client, url: &str, origins: &[String]) -> Result<Value> {
    trusted(url, origins).await?;
    let mut r = checked(c.get(url).send().await?).await?;
    let mut bytes = Vec::new();
    while let Some(chunk) = r.chunk().await? {
        if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
            bail!("JSON превышает 8 МБ")
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&bytes)?)
}
pub fn sha256(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}
pub fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("Нет родительского каталога")?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
pub fn signed(envelope: Value, key: &str) -> Result<Value> {
    let bytes: [u8; 32] = STANDARD
        .decode(key)
        .context("Публичный ключ не настроен")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Ключ должен быть Ed25519 (32 байта)"))?;
    let sig = STANDARD.decode(
        envelope["signature"]
            .as_str()
            .context("Нет подписи manifest")?,
    )?;
    let data = envelope.get("payload").context("Нет payload")?.clone();
    // serde_json uses a sorted BTreeMap; compact UTF-8 matches the administrator signer.
    VerifyingKey::from_bytes(&bytes)?
        .verify(&serde_json::to_vec(&data)?, &Signature::from_slice(&sig)?)?;
    Ok(data)
}
pub async fn download(
    c: &Client,
    url: &str,
    origins: &[String],
    target: &Path,
    size: u64,
    hash: &str,
    sha1_hash: bool,
    mut progress: impl FnMut(u64),
) -> Result<()> {
    trusted(url, origins).await?;
    transfer(c, url, target, size, hash, sha1_hash, &mut progress).await
}
async fn transfer(
    c: &Client,
    url: &str,
    target: &Path,
    size: u64,
    hash: &str,
    sha1_hash: bool,
    mut progress: impl FnMut(u64),
) -> Result<()> {
    fs::create_dir_all(target.parent().unwrap())?;
    let mut last = None;
    for attempt in 0..3 {
        let result: Result<()> = async {
            let mut r = checked(c.get(url).send().await?).await?;
            let mut temp = tempfile::NamedTempFile::new_in(target.parent().unwrap())?;
            let mut count = 0;
            let mut h256 = Sha256::new();
            let mut h1 = sha1::Sha1::new();
            while let Some(chunk) = r.chunk().await? {
                count += chunk.len() as u64;
                if count > size {
                    bail!("Файл больше заявленного размера")
                }
                temp.write_all(&chunk)?;
                h256.update(&chunk);
                h1.update(&chunk);
                progress(count);
            }
            let actual = if sha1_hash {
                hex::encode(h1.finalize())
            } else {
                hex::encode(h256.finalize())
            };
            if count != size || actual != hash {
                bail!("Не совпадает размер или контрольная сумма: {url}")
            }
            temp.as_file().sync_all()?;
            temp.persist(target).map_err(|e| e.error)?;
            Ok(())
        }
        .await;
        match result {
            Ok(_) => return Ok(()),
            Err(e) => last = Some(e),
        }
        tokio::time::sleep(Duration::from_millis(300 * (attempt + 1))).await;
    }
    Err(last.unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[test]
    fn denies_insecure_sources() {
        for s in [
            "http://cdn.example/x",
            "https://a:b@cdn.example/x",
            "https://cdn.example/x#part",
        ] {
            assert!(origin(s).is_err())
        }
        assert_eq!(
            origin("https://cdn.example/x").unwrap(),
            "https://cdn.example"
        );
    }
    #[tokio::test]
    async fn denies_private_sources() {
        assert!(
            trusted("https://127.0.0.1/x", &["https://127.0.0.1".into()])
                .await
                .is_err()
        );
        assert!(trusted("https://not-trusted.example/x", &[]).await.is_err());
    }
    #[test]
    fn verifies_signature_and_rejects_tampering() {
        use ed25519_dalek::{Signer, SigningKey};
        let key = SigningKey::from_bytes(&[42; 32]);
        let payload = serde_json::json!({"version":"1","files":[]});
        let signature =
            STANDARD.encode(key.sign(&serde_json::to_vec(&payload).unwrap()).to_bytes());
        let public = STANDARD.encode(key.verifying_key().to_bytes());
        let envelope = serde_json::json!({"payload":payload,"signature":signature});
        assert!(signed(envelope.clone(), &public).is_ok());
        let mut changed = envelope;
        changed["payload"]["version"] = Value::String("2".into());
        assert!(signed(changed, &public).is_err());
    }
    async fn fixture(bodies: Vec<Vec<u8>>) -> (String, tokio::task::JoinHandle<()>) {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/file", l.local_addr().unwrap());
        let task = tokio::spawn(async move {
            for body in bodies {
                let (mut socket, _) = l.accept().await.unwrap();
                let mut buf = [0; 4096];
                let _ = socket.read(&mut buf).await;
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                socket.write_all(header.as_bytes()).await.unwrap();
                socket.write_all(&body).await.unwrap();
            }
        });
        (url, task)
    }
    #[tokio::test]
    async fn downloads_and_retries_corrupt_bytes() {
        let (url, task) = fixture(vec![b"wrong".to_vec(), b"hello".to_vec()]).await;
        let d = tempfile::tempdir().unwrap();
        let out = d.path().join("x");
        let hash = hex::encode(Sha256::digest(b"hello"));
        let mut progress = 0;
        transfer(
            &Client::builder().no_proxy().build().unwrap(),
            &url,
            &out,
            5,
            &hash,
            false,
            |n| progress = n,
        )
        .await
        .unwrap();
        assert_eq!(fs::read(out).unwrap(), b"hello");
        assert_eq!(progress, 5);
        task.await.unwrap();
    }
    #[tokio::test]
    async fn failed_download_preserves_previous_file_and_cleans_temp() {
        let (url, task) = fixture(vec![b"bad".to_vec(); 3]).await;
        let d = tempfile::tempdir().unwrap();
        let out = d.path().join("x");
        fs::write(&out, b"previous").unwrap();
        assert!(transfer(
            &Client::builder().no_proxy().build().unwrap(),
            &url,
            &out,
            5,
            &hex::encode(Sha256::digest(b"hello")),
            false,
            |_| {}
        )
        .await
        .is_err());
        assert_eq!(fs::read(&out).unwrap(), b"previous");
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 1);
        task.await.unwrap();
    }
}
