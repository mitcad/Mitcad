// SPDX-License-Identifier: MIT
//! MQTT 3.1.1 packets (OASIS standard, 2014): their encoding, and a
//! decoder that checks every packet before it is used.
//!
//! Only what Mitcad uses is supported: QoS 0 and 1 (a QoS 2 packet is an
//! error), MQTT 3.1.1 only (protocol level 4). The decoder reads a stream
//! of bytes incrementally and refuses a packet whose remaining length is
//! larger than its limit as soon as the length is read, before any of the
//! packet's body is buffered, so a broker cannot make it hold more than the
//! limit and one read.

use std::fmt;

/// A packet's type in the fixed header.
const CONNECT: u8 = 1;
const CONNACK: u8 = 2;
const PUBLISH: u8 = 3;
const PUBACK: u8 = 4;
const SUBSCRIBE: u8 = 8;
const SUBACK: u8 = 9;
const UNSUBSCRIBE: u8 = 10;
const UNSUBACK: u8 = 11;
const PINGREQ: u8 = 12;
const PINGRESP: u8 = 13;
const DISCONNECT: u8 = 14;

/// The largest remaining length the encoding can express (four bytes).
const MAX_ENCODABLE: usize = 268_435_455;

/// What is wrong with a packet. Any of these ends the connection: the
/// stream cannot be trusted after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketError {
    /// The remaining length is larger than the decoder's limit.
    TooLarge(usize),
    /// Not a valid MQTT 3.1.1 packet, or one Mitcad does not use.
    Malformed(&'static str),
}

impl fmt::Display for PacketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PacketError::TooLarge(n) => write!(f, "a packet of {n} bytes is larger than allowed"),
            PacketError::Malformed(why) => write!(f, "a malformed packet: {why}"),
        }
    }
}

impl std::error::Error for PacketError {}

fn malformed<T>(why: &'static str) -> Result<T, PacketError> {
    Err(PacketError::Malformed(why))
}

/// A will: what the broker publishes when the connection ends without a
/// DISCONNECT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Will {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connect {
    pub client_id: String,
    /// Seconds.
    pub keep_alive: u16,
    pub clean_session: bool,
    pub will: Option<Will>,
    pub user: Option<String>,
    pub password: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publish {
    pub topic: String,
    pub payload: Vec<u8>,
    /// 0 or 1.
    pub qos: u8,
    pub retain: bool,
    /// A QoS 1 message sent again.
    pub dup: bool,
    /// The packet identifier of a QoS 1 message (1 to 65535); 0 for QoS 0.
    pub id: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    Connect(Connect),
    ConnAck {
        session_present: bool,
        /// 0: accepted; 1 to 5: refused (the protocol version, the client
        /// identifier, the server unavailable, a bad user name or password,
        /// not authorised).
        code: u8,
    },
    Publish(Publish),
    PubAck(u16),
    Subscribe {
        id: u16,
        /// Topic filters with the largest QoS asked for.
        filters: Vec<(String, u8)>,
    },
    SubAck {
        id: u16,
        /// The QoS granted per filter, or 0x80 for a refused one.
        codes: Vec<u8>,
    },
    Unsubscribe {
        id: u16,
        filters: Vec<String>,
    },
    UnsubAck(u16),
    PingReq,
    PingResp,
    Disconnect,
}

impl Packet {
    /// The packet's bytes. Fails only for fields MQTT cannot carry (a
    /// string longer than 65535 bytes, a packet over 256 MiB).
    pub fn encode(&self) -> Result<Vec<u8>, PacketError> {
        let mut body = Vec::new();
        let (kind, flags) = match self {
            Packet::Connect(c) => {
                put_str(&mut body, "MQTT")?;
                body.push(4);
                let mut flags = 0u8;
                if c.user.is_some() {
                    flags |= 0x80;
                }
                if c.password.is_some() {
                    flags |= 0x40;
                }
                if let Some(will) = &c.will {
                    flags |= 0x04 | (will.qos.min(1) << 3);
                    if will.retain {
                        flags |= 0x20;
                    }
                }
                if c.clean_session {
                    flags |= 0x02;
                }
                body.push(flags);
                body.extend_from_slice(&c.keep_alive.to_be_bytes());
                put_str(&mut body, &c.client_id)?;
                if let Some(will) = &c.will {
                    put_str(&mut body, &will.topic)?;
                    put_bytes(&mut body, &will.payload)?;
                }
                if let Some(user) = &c.user {
                    put_str(&mut body, user)?;
                }
                if let Some(password) = &c.password {
                    put_bytes(&mut body, password)?;
                }
                (CONNECT, 0)
            }
            Packet::ConnAck {
                session_present,
                code,
            } => {
                body.push(u8::from(*session_present));
                body.push(*code);
                (CONNACK, 0)
            }
            Packet::Publish(p) => {
                put_str(&mut body, &p.topic)?;
                if p.qos > 0 {
                    body.extend_from_slice(&p.id.to_be_bytes());
                }
                body.extend_from_slice(&p.payload);
                let flags = (u8::from(p.dup) << 3) | (p.qos.min(1) << 1) | u8::from(p.retain);
                (PUBLISH, flags)
            }
            Packet::PubAck(id) => {
                body.extend_from_slice(&id.to_be_bytes());
                (PUBACK, 0)
            }
            Packet::Subscribe { id, filters } => {
                body.extend_from_slice(&id.to_be_bytes());
                for (filter, qos) in filters {
                    put_str(&mut body, filter)?;
                    body.push(*qos);
                }
                (SUBSCRIBE, 0b0010)
            }
            Packet::SubAck { id, codes } => {
                body.extend_from_slice(&id.to_be_bytes());
                body.extend_from_slice(codes);
                (SUBACK, 0)
            }
            Packet::Unsubscribe { id, filters } => {
                body.extend_from_slice(&id.to_be_bytes());
                for filter in filters {
                    put_str(&mut body, filter)?;
                }
                (UNSUBSCRIBE, 0b0010)
            }
            Packet::UnsubAck(id) => {
                body.extend_from_slice(&id.to_be_bytes());
                (UNSUBACK, 0)
            }
            Packet::PingReq => (PINGREQ, 0),
            Packet::PingResp => (PINGRESP, 0),
            Packet::Disconnect => (DISCONNECT, 0),
        };
        if body.len() > MAX_ENCODABLE {
            return Err(PacketError::TooLarge(body.len()));
        }
        let mut packet = Vec::with_capacity(body.len() + 5);
        packet.push(kind << 4 | flags);
        put_length(&mut packet, body.len());
        packet.extend_from_slice(&body);
        Ok(packet)
    }
}

fn put_length(out: &mut Vec<u8>, mut length: usize) {
    loop {
        let mut byte = (length % 128) as u8;
        length /= 128;
        if length > 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if length == 0 {
            break;
        }
    }
}

fn put_bytes(out: &mut Vec<u8>, data: &[u8]) -> Result<(), PacketError> {
    let length = u16::try_from(data.len()).or(malformed("a field longer than 65535 bytes"))?;
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(data);
    Ok(())
}

fn put_str(out: &mut Vec<u8>, text: &str) -> Result<(), PacketError> {
    put_bytes(out, text.as_bytes())
}

/// Reads packets from a stream of bytes as they arrive.
#[derive(Debug)]
pub struct Decoder {
    buffer: Vec<u8>,
    limit: usize,
}

impl Decoder {
    /// A decoder of packets whose remaining length is at most `limit`.
    pub fn new(limit: usize) -> Self {
        Self {
            buffer: Vec::new(),
            limit,
        }
    }

    /// Adds bytes read from the stream. Call [`Decoder::next_packet`] until it
    /// gives None before adding more, so that the buffer stays within the
    /// limit and one read.
    pub fn push(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// The bytes buffered and not decoded yet.
    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }

    /// The next whole packet, None when more bytes are needed.
    pub fn next_packet(&mut self) -> Result<Option<Packet>, PacketError> {
        let Some((length, header)) = remaining_length(&self.buffer)? else {
            return Ok(None);
        };
        if length > self.limit {
            return Err(PacketError::TooLarge(length));
        }
        if self.buffer.len() < header + length {
            return Ok(None);
        }
        let packet = decode(self.buffer[0], &self.buffer[header..header + length]);
        self.buffer.drain(..header + length);
        packet.map(Some)
    }
}

/// The remaining length and the size of the fixed header, None while the
/// length is incomplete.
fn remaining_length(buffer: &[u8]) -> Result<Option<(usize, usize)>, PacketError> {
    let mut length = 0usize;
    for i in 0..4 {
        let Some(&byte) = buffer.get(1 + i) else {
            return Ok(None);
        };
        length |= usize::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            // The shortest encoding only: a last byte of 0 after others
            // would make one length readable in several ways.
            if i > 0 && byte == 0 {
                return malformed("a remaining length not in its shortest form");
            }
            return Ok(Some((length, 2 + i)));
        }
    }
    malformed("a remaining length of more than four bytes")
}

/// Decodes one packet from its first byte and its body (the remaining
/// length's bytes).
pub fn decode(first: u8, body: &[u8]) -> Result<Packet, PacketError> {
    let kind = first >> 4;
    let flags = first & 0x0f;
    let mut r = Reader { data: body, at: 0 };
    let fixed_flags = |expected: u8| {
        if flags == expected {
            Ok(())
        } else {
            malformed("reserved flags of the fixed header are wrong")
        }
    };
    let packet = match kind {
        CONNECT => {
            fixed_flags(0)?;
            Packet::Connect(decode_connect(&mut r)?)
        }
        CONNACK => {
            fixed_flags(0)?;
            let ack = r.byte()?;
            let code = r.byte()?;
            if ack & 0xfe != 0 {
                return malformed("reserved CONNACK flags are set");
            }
            if code > 5 {
                return malformed("an unknown CONNACK return code");
            }
            Packet::ConnAck {
                session_present: ack == 1,
                code,
            }
        }
        PUBLISH => {
            let qos = (flags >> 1) & 0x03;
            let dup = flags & 0x08 != 0;
            let retain = flags & 0x01 != 0;
            match qos {
                0 if dup => return malformed("a QoS 0 message marked as sent again"),
                0 | 1 => {}
                2 => return malformed("QoS 2 is not used"),
                _ => return malformed("QoS 3 does not exist"),
            }
            let topic = r.string()?;
            if !valid_topic_name(&topic) {
                return malformed("a topic name that is empty or has wildcards");
            }
            let id = if qos > 0 { r.packet_id()? } else { 0 };
            Packet::Publish(Publish {
                topic,
                payload: r.rest().to_vec(),
                qos,
                retain,
                dup,
                id,
            })
        }
        PUBACK => {
            fixed_flags(0)?;
            Packet::PubAck(r.packet_id()?)
        }
        SUBSCRIBE => {
            fixed_flags(0b0010)?;
            let id = r.packet_id()?;
            let mut filters = Vec::new();
            while !r.is_empty() {
                let filter = r.string()?;
                if !valid_topic_filter(&filter) {
                    return malformed("a topic filter with misplaced wildcards");
                }
                let qos = r.byte()?;
                if qos > 2 {
                    return malformed("a subscription asks for a QoS that does not exist");
                }
                filters.push((filter, qos));
            }
            if filters.is_empty() {
                return malformed("a SUBSCRIBE without topic filters");
            }
            Packet::Subscribe { id, filters }
        }
        SUBACK => {
            fixed_flags(0)?;
            let id = r.packet_id()?;
            let codes = r.rest().to_vec();
            if codes.is_empty() {
                return malformed("a SUBACK without return codes");
            }
            if codes.iter().any(|c| !matches!(c, 0 | 1 | 2 | 0x80)) {
                return malformed("an unknown SUBACK return code");
            }
            Packet::SubAck { id, codes }
        }
        UNSUBSCRIBE => {
            fixed_flags(0b0010)?;
            let id = r.packet_id()?;
            let mut filters = Vec::new();
            while !r.is_empty() {
                let filter = r.string()?;
                if !valid_topic_filter(&filter) {
                    return malformed("a topic filter with misplaced wildcards");
                }
                filters.push(filter);
            }
            if filters.is_empty() {
                return malformed("an UNSUBSCRIBE without topic filters");
            }
            Packet::Unsubscribe { id, filters }
        }
        UNSUBACK => {
            fixed_flags(0)?;
            Packet::UnsubAck(r.packet_id()?)
        }
        PINGREQ => {
            fixed_flags(0)?;
            Packet::PingReq
        }
        PINGRESP => {
            fixed_flags(0)?;
            Packet::PingResp
        }
        DISCONNECT => {
            fixed_flags(0)?;
            Packet::Disconnect
        }
        5..=7 => return malformed("QoS 2 is not used"),
        _ => return malformed("a reserved packet type"),
    };
    if !r.is_empty() {
        return malformed("bytes after the end of the packet");
    }
    Ok(packet)
}

fn decode_connect(r: &mut Reader<'_>) -> Result<Connect, PacketError> {
    if r.string()? != "MQTT" {
        return malformed("not the MQTT protocol");
    }
    if r.byte()? != 4 {
        return malformed("not MQTT 3.1.1");
    }
    let flags = r.byte()?;
    if flags & 0x01 != 0 {
        return malformed("the reserved CONNECT flag is set");
    }
    let keep_alive = r.u16()?;
    let will_flag = flags & 0x04 != 0;
    let will_qos = (flags >> 3) & 0x03;
    let will_retain = flags & 0x20 != 0;
    if !will_flag && (will_qos != 0 || will_retain) {
        return malformed("will settings without a will");
    }
    if will_qos > 1 {
        return malformed("a will QoS of 2 or more");
    }
    let has_user = flags & 0x80 != 0;
    let has_password = flags & 0x40 != 0;
    if has_password && !has_user {
        return malformed("a password without a user name");
    }
    let client_id = r.string()?;
    let will = if will_flag {
        let topic = r.string()?;
        if !valid_topic_name(&topic) {
            return malformed("a will topic that is empty or has wildcards");
        }
        Some(Will {
            topic,
            payload: r.bytes()?.to_vec(),
            qos: will_qos,
            retain: will_retain,
        })
    } else {
        None
    };
    let user = has_user.then(|| r.string()).transpose()?;
    let password = has_password
        .then(|| r.bytes().map(<[u8]>::to_vec))
        .transpose()?;
    Ok(Connect {
        client_id,
        keep_alive,
        clean_session: flags & 0x02 != 0,
        will,
        user,
        password,
    })
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn is_empty(&self) -> bool {
        self.at >= self.data.len()
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], PacketError> {
        let end = self.at.checked_add(n).filter(|&end| end <= self.data.len());
        match end {
            Some(end) => {
                let slice = &self.data[self.at..end];
                self.at = end;
                Ok(slice)
            }
            None => malformed("the packet ends early"),
        }
    }

    fn byte(&mut self) -> Result<u8, PacketError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, PacketError> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn packet_id(&mut self) -> Result<u16, PacketError> {
        match self.u16()? {
            0 => malformed("a packet identifier of 0"),
            id => Ok(id),
        }
    }

    fn bytes(&mut self) -> Result<&'a [u8], PacketError> {
        let length = usize::from(self.u16()?);
        self.take(length)
    }

    /// A UTF-8 string without U+0000 (MQTT 3.1.1, 1.5.3).
    fn string(&mut self) -> Result<String, PacketError> {
        let bytes = self.bytes()?;
        match std::str::from_utf8(bytes) {
            Ok(text) if !text.contains('\0') => Ok(text.to_owned()),
            Ok(_) => malformed("a string with U+0000"),
            Err(_) => malformed("a string that is not UTF-8"),
        }
    }

    fn rest(&mut self) -> &'a [u8] {
        let rest = &self.data[self.at.min(self.data.len())..];
        self.at = self.data.len();
        rest
    }
}

/// A topic name a message is published to: not empty, no wildcards.
pub fn valid_topic_name(topic: &str) -> bool {
    !topic.is_empty() && !topic.contains(['+', '#'])
}

/// A topic filter: `+` alone in its level, `#` alone in the last one.
pub fn valid_topic_filter(filter: &str) -> bool {
    if filter.is_empty() {
        return false;
    }
    let levels: Vec<&str> = filter.split('/').collect();
    levels.iter().enumerate().all(|(i, level)| match *level {
        "#" => i + 1 == levels.len(),
        "+" => true,
        level => !level.contains(['+', '#']),
    })
}
