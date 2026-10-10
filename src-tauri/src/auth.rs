use crate::{net, Settings};
use anyhow::{bail, Result};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub id: String,
    pub access_token: String,
    pub xuid: String,
}
pub fn offline(name: &str) -> Result<Account> {
    if !(3..=16).contains(&name.len())
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        bail!("Укажите ник: 3–16 латинских букв, цифр или подчёркиваний")
    }
    let mut hash: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    hash[6] = (hash[6] & 0x0f) | 0x30;
    hash[8] = (hash[8] & 0x3f) | 0x80;
    Ok(Account {
        name: name.into(),
        id: hex::encode(hash),
        access_token: "0".into(),
        xuid: String::new(),
    })
}
async fn answer(r: reqwest::Response) -> Result<Value> {
    if !r.status().is_success() {
        let status = r.status();
        let value = r.json::<Value>().await.unwrap_or(json!({}));
        let detail = value["detail"]
            .as_str()
            .or(value["error_description"].as_str())
            .unwrap_or("Ошибка авторизации");
        bail!("HTTP {status}: {detail}");
    }
    Ok(r.json().await?)
}
pub async fn public_api(s: &Settings, path: &str) -> Result<Value> {
    let base = &s.deployment.api_url;
    if base.is_empty() {
        bail!("Источник новостей и статуса сервера не настроен")
    }
    let origins = vec![base.clone()];
    let url = format!("{}{}", base.trim_end_matches('/'), path);
    net::trusted(&url, &origins).await?;
    answer(net::client()?.get(&url).send().await?).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_offline_uuid() {
        assert_eq!(
            offline("Notch").unwrap().id,
            "b50ad385829d3141a2167e7d7539ba7f"
        );
        assert_ne!(offline("Notch").unwrap().id, offline("notch").unwrap().id);
    }
    #[test]
    fn rejects_invalid_nickname() {
        for name in ["", "ab", "abcdefghijklmnopq", "a b", "Игрок", "abc/def"] {
            assert!(offline(name).is_err());
        }
        assert!(offline("StudHub_01").is_ok());
    }
}
