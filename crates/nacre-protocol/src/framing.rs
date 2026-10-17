//! Bounded UTF-8 JSONL with duplicate-key rejection, including nested payloads.
use crate::{ErrorCode, Failure, Message, MAX_FRAME};
use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::{Map, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

fn invalid() -> Failure {
    Failure::new(ErrorCode::InvalidRequest, "Invalid protocol frame", false)
}
fn large() -> Failure {
    Failure::new(ErrorCode::TooLarge, "Protocol frame exceeds 1 MiB", false)
}
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Values;
        impl<'de> Visitor<'de> for Values {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|v| Unique(Value::Number(v)))
                    .ok_or_else(|| E::custom("Nonfinite number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Unique, A::Error> {
                let mut out = Vec::new();
                while let Some(Unique(v)) = seq.next_element()? {
                    out.push(v);
                }
                Ok(Unique(Value::Array(out)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Unique, A::Error> {
                let mut out = Map::new();
                while let Some((key, Unique(value))) = map.next_entry::<String, Unique>()? {
                    if out.contains_key(&key) {
                        return Err(de::Error::custom("Duplicate object key"));
                    }
                    out.insert(key, value);
                }
                Ok(Unique(Value::Object(out)))
            }
        }
        d.deserialize_any(Values)
    }
}
pub fn decode(bytes: &[u8]) -> Result<Message, Failure> {
    if bytes.len() > MAX_FRAME {
        return Err(large());
    }
    let Unique(value) = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if value.get("t") == Some(&Value::String("ret".into()))
        && (value.get("ok").is_some() == value.get("err").is_some())
    {
        return Err(invalid());
    }
    let message: Message = serde_json::from_value(value).map_err(|_| invalid())?;
    message.validate()?;
    Ok(message)
}
pub fn encode(message: &Message) -> Result<Vec<u8>, Failure> {
    message.validate()?;
    let mut bytes = serde_json::to_vec(message).map_err(|_| invalid())?;
    if bytes.len() > MAX_FRAME {
        return Err(large());
    }
    bytes.push(b'\n');
    Ok(bytes)
}
#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
}
impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Message>, Failure> {
        let mut messages = Vec::new();
        for chunk in bytes.split_inclusive(|b| *b == b'\n') {
            let ended = chunk.last() == Some(&b'\n');
            let content = if ended {
                &chunk[..chunk.len() - 1]
            } else {
                chunk
            };
            if self.buffer.len() + content.len() > MAX_FRAME {
                self.buffer.clear();
                return Err(large());
            }
            self.buffer.extend_from_slice(content);
            if ended {
                messages.push(decode(&self.buffer)?);
                self.buffer.clear();
            }
        }
        Ok(messages)
    }
    pub fn pending_bytes(&self) -> usize {
        self.buffer.len()
    }
}
/// Cancellation-safe buffered reader: a dropped next() future retains partial bytes.
pub struct Reader<R> {
    reader: R,
    line: Vec<u8>,
    complete: bool,
}
impl<R: AsyncBufRead + Unpin> Reader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            line: Vec::new(),
            complete: false,
        }
    }
    /// Capacity of this reader's line buffer, excluding the underlying reader.
    pub fn retained_bytes(&self) -> usize {
        self.line.capacity()
    }
    /// Release a completed/empty frame's allocation when above the ceiling.
    /// A cancelled, unfinished frame is never discarded. Completed raw bytes
    /// can be released only after their borrow ends. This does not promise an
    /// allocator or process RSS reduction.
    pub fn trim_idle(&mut self, max_bytes: usize) -> bool {
        if !self.complete && !self.line.is_empty() {
            return false;
        }
        if self.line.capacity() <= max_bytes {
            return false;
        }
        self.line = Vec::new();
        self.complete = false;
        true
    }
    async fn line(&mut self) -> Result<bool, Failure> {
        if self.complete {
            self.line.clear();
            self.complete = false;
        }
        loop {
            let data = self
                .reader
                .fill_buf()
                .await
                .map_err(|_| Failure::new(ErrorCode::Io, "Protocol read failed", true))?;
            if data.is_empty() {
                return if self.line.is_empty() {
                    Ok(false)
                } else {
                    Err(invalid())
                };
            }
            let newline = data.iter().position(|b| *b == b'\n');
            let used = newline.map_or(data.len(), |n| n + 1);
            let content = used - usize::from(newline.is_some());
            if self.line.len() + content > MAX_FRAME {
                return Err(large());
            }
            self.line.extend_from_slice(&data[..content]);
            self.reader.consume(used);
            if newline.is_some() {
                self.complete = true;
                return Ok(true);
            }
        }
    }
    pub async fn next(&mut self) -> Result<Option<Message>, Failure> {
        if self.line().await? {
            decode(&self.line).map(Some)
        } else {
            Ok(None)
        }
    }
    /// Validated original bytes, excluding LF. Bridges retain unknown optional
    /// fields rather than dropping them during typed reserialization. The buffer
    /// is reused on the next read, including after a cancelled partial read.
    pub async fn next_raw(&mut self) -> Result<Option<&[u8]>, Failure> {
        if !self.line().await? {
            return Ok(None);
        }
        decode(&self.line)?;
        Ok(Some(&self.line))
    }
}
/// Convenience for reads that are not resumed after cancellation.
pub async fn read<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Option<Message>, Failure> {
    Reader::new(reader).next().await
}

pub async fn write<W: AsyncWrite + Unpin>(
    writer: &mut W,
    message: &Message,
) -> Result<(), Failure> {
    writer
        .write_all(&encode(message)?)
        .await
        .map_err(|_| Failure::new(ErrorCode::Io, "Protocol write failed", true))?;
    writer
        .flush()
        .await
        .map_err(|_| Failure::new(ErrorCode::Io, "Protocol flush failed", true))
}
