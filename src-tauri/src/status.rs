use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn varint(mut n: u32) -> Vec<u8> {
    let mut v = Vec::new();
    loop {
        let b = (n & 127) as u8;
        n >>= 7;
        v.push(if n == 0 { b } else { b | 128 });
        if n == 0 {
            return v;
        }
    }
}
async fn read_varint(s: &mut tokio::net::TcpStream) -> Result<u32> {
    let mut value = 0;
    for shift in (0..35).step_by(7) {
        let b = s.read_u8().await?;
        value |= ((b & 127) as u32) << shift;
        if b & 128 == 0 {
            return Ok(value);
        }
    }
    bail!("Некорректный ответ сервера")
}
pub async fn ping(address: &str) -> Result<Value> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let (host,port)=address.rsplit_once(':').context("Укажите адрес:порт")?;
        let port:u16=port.parse()?;
        let mut s=tokio::net::TcpStream::connect((host,port)).await?;
        let mut handshake=vec![0];
        handshake.extend(varint(767)); // Minecraft 1.21.1
        handshake.extend(varint(host.len() as u32));
        handshake.extend(host.as_bytes());
        handshake.extend(port.to_be_bytes());
        handshake.push(1);
        s.write_all(&varint(handshake.len() as u32)).await?;
        s.write_all(&handshake).await?;
        s.write_all(&[1,0]).await?;
        let size=read_varint(&mut s).await?;
        if size>1024*1024 || size<3 { bail!("Некорректный размер ответа"); }
        let mut packet=vec![0;size as usize]; s.read_exact(&mut packet).await?;
        if packet[0]!=0 { bail!("Неизвестный пакет статуса"); }
        let mut len=0u32; let mut offset=1;
        for shift in (0..35).step_by(7) {
            let b=*packet.get(offset).context("Неполный ответ")?; offset+=1;
            len|=((b&127) as u32)<<shift; if b&128==0 {break;}
        }
        let data=packet.get(offset..offset+len as usize).context("Неполный JSON")?;
        let v:Value=serde_json::from_slice(data)?;
        Ok(json!({"online":true,"players":v["players"]["online"],"max_players":v["players"]["max"],"version":v["version"]["name"]}))
    }).await.context("Сервер не ответил за 5 секунд")?
}

pub fn spawn_count(value: &Value, now: u64) -> Result<u64> {
    let count = value["players"]
        .as_u64()
        .context("Нет числа игроков на спавне")?;
    let time = value["updated_at"]
        .as_u64()
        .context("Нет времени подсчёта спавна")?;
    if count > 1_000_000 || time > now.saturating_add(10) || now.saturating_sub(time) > 30 {
        bail!("Данные спавна устарели или некорректны")
    }
    Ok(count)
}
pub async fn spawn(url: &str, origins: &[String]) -> Result<u64> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let value = crate::net::json(&crate::net::client()?, url, origins).await?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        spawn_count(&value, now)
    })
    .await
    .context("Счётчик спавна не ответил за 5 секунд")?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spawn_requires_fresh_real_count() {
        assert_eq!(
            spawn_count(&json!({"players":0,"updated_at":100}), 100).unwrap(),
            0
        );
        for v in [
            json!({"players":3}),
            json!({"players":-1,"updated_at":100}),
            json!({"players":3,"updated_at":10}),
            json!({"players":3,"updated_at":200}),
        ] {
            assert!(spawn_count(&v, 100).is_err());
        }
    }
}
