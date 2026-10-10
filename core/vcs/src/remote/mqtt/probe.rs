// SPDX-License-Identifier: MIT
//! The `test` command (Project Settings' Test button): a connection of its
//! own that connects, signs in, subscribes to a topic under the prefix,
//! publishes once to it and waits for the message to come back, then
//! disconnects; each step timed. It blocks, so the application runs it on
//! a worker thread.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::check;
use super::connection::read_packet;
use super::hub::Target;
use super::packet::{Connect, Decoder, Packet, Publish};
use super::transport::{Transport, hex, io_error, random_bytes};
use super::{LiveError, LiveErrorClass, MAX_PACKET};

/// The format of the test message.
const TEST_FORMAT: &str = "mitcad-live-test";

struct Probe {
    transport: Transport,
    decoder: Decoder,
    topic: String,
    payload: Vec<u8>,
    echoed: bool,
}

impl Probe {
    fn send(&mut self, packet: &Packet) -> Result<(), LiveError> {
        let bytes = packet
            .encode()
            .map_err(|e| LiveError::new(LiveErrorClass::Internal, e.to_string()))?;
        self.transport
            .write_all(&bytes)
            .map_err(|e| io_error("cannot send to the broker", &e))
    }

    /// Reads packets until `wanted` takes one, acknowledging and noting
    /// what is published meanwhile.
    fn expect<T>(
        &mut self,
        deadline: Instant,
        mut wanted: impl FnMut(&Packet, bool) -> Option<Result<T, LiveError>>,
    ) -> Result<T, LiveError> {
        loop {
            let packet = read_packet(&mut self.transport, &mut self.decoder, deadline)?;
            if let Packet::Publish(publish) = &packet {
                if publish.qos == 1 {
                    self.send(&Packet::PubAck(publish.id))?;
                }
                if publish.topic == self.topic && publish.payload == self.payload {
                    self.echoed = true;
                }
            }
            if let Some(result) = wanted(&packet, self.echoed) {
                return result;
            }
        }
    }
}

fn step<T>(
    steps: &mut Vec<Value>,
    name: &str,
    f: impl FnOnce() -> Result<T, LiveError>,
) -> Result<T, LiveError> {
    let started = Instant::now();
    let result = f();
    steps.push(json!({
        "step": name,
        "ok": result.is_ok(),
        "ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    }));
    result
}

/// Runs the test; the answer says how far it got.
pub fn run(target: &Target, timeout: Duration) -> Value {
    let topic = format!("{}/test/{}", target.prefix, hex(&random_bytes::<8>()));
    let mut steps = Vec::new();
    let result = probe(target, timeout, &topic, &mut steps);
    json!({
        "ok": result.is_ok(),
        "broker": target.broker.to_string(),
        "prefix": target.prefix,
        "user": target.user,
        "tls": target.broker.tls,
        "topic": topic,
        "steps": steps,
        "error": result.err(),
        "warning": target.warning(),
    })
}

/// The test's answer as text for people (`mitcad-cli live test`).
pub fn describe(answer: &Value) -> String {
    let text = |key: &str| answer[key].as_str().unwrap_or_default().to_owned();
    let mut out = format!(
        "Live updates through {} (prefix {}",
        text("broker"),
        text("prefix")
    );
    if let Some(user) = answer["user"].as_str() {
        out.push_str(&format!(", user {user}"));
    }
    out.push_str(")\n");
    for step in answer["steps"].as_array().into_iter().flatten() {
        let ok = if step["ok"] == true { "ok" } else { "failed" };
        out.push_str(&format!(
            "  {}: {ok} ({} ms)\n",
            step["step"].as_str().unwrap_or_default(),
            step["ms"]
        ));
    }
    if let Some(warning) = answer["warning"].as_str() {
        out.push_str(&format!("Warning: {warning}.\n"));
    }
    match answer["error"]["message"].as_str() {
        Some(message) => out.push_str(&format!("Failed: {message}\n")),
        None => out.push_str(&format!(
            "Works: a message published to {} came back.\n",
            text("topic")
        )),
    }
    out
}

fn probe(
    target: &Target,
    timeout: Duration,
    topic: &str,
    steps: &mut Vec<Value>,
) -> Result<(), LiveError> {
    let transport = step(steps, "connect", || {
        Transport::open(&target.broker, &target.authorities, timeout)
    })?;
    let payload = serde_json::to_vec(&json!({
        "format": TEST_FORMAT,
        "version": 1,
        "at": check::format_time(check::now()),
    }))
    .expect("serializable");
    let mut probe = Probe {
        transport,
        decoder: Decoder::new(MAX_PACKET),
        topic: topic.to_owned(),
        payload,
        echoed: false,
    };
    let deadline = || Instant::now() + timeout;
    step(steps, "sign_in", || {
        probe.send(&Packet::Connect(Connect {
            client_id: format!("mitcad{}", &hex(&random_bytes::<9>())[..17]),
            keep_alive: 30,
            clean_session: true,
            will: None,
            user: target.user.clone(),
            password: target.password.as_ref().map(|p| p.as_bytes().to_vec()),
        }))?;
        probe.expect(deadline(), |packet, _| match packet {
            Packet::ConnAck { code: 0, .. } => Some(Ok(())),
            Packet::ConnAck { code, .. } => Some(Err(super::connection::refused(*code))),
            _ => Some(Err(LiveError::new(
                LiveErrorClass::Protocol,
                "the broker did not answer the connection as MQTT 3.1.1 does",
            ))),
        })
    })?;
    step(steps, "subscribe", || {
        probe.send(&Packet::Subscribe {
            id: 1,
            filters: vec![(topic.to_owned(), 1)],
        })?;
        probe.expect(deadline(), |packet, _| match packet {
            Packet::SubAck { id: 1, codes } if codes.contains(&0x80) => Some(Err(LiveError::new(
                LiveErrorClass::AuthFailed,
                format!("the broker refused the subscription to {topic}"),
            ))),
            Packet::SubAck { id: 1, .. } => Some(Ok(())),
            _ => None,
        })
    })?;
    step(steps, "publish", || {
        let publish = Packet::Publish(Publish {
            topic: topic.to_owned(),
            payload: probe.payload.clone(),
            qos: 1,
            retain: false,
            dup: false,
            id: 2,
        });
        probe.send(&publish)?;
        probe.expect(deadline(), |packet, _| match packet {
            Packet::PubAck(2) => Some(Ok(())),
            _ => None,
        })
    })?;
    step(steps, "receive", || {
        if probe.echoed {
            return Ok(());
        }
        probe.expect(deadline(), |_, echoed| echoed.then_some(Ok(())))
    })
    .map_err(|e| match e.class {
        LiveErrorClass::TimedOut => LiveError::new(
            LiveErrorClass::AuthFailed,
            format!(
                "the message published to {topic} did not come back: the broker may not let \
                 this user read or write under the prefix"
            ),
        ),
        _ => e,
    })?;
    let _ = probe.send(&Packet::Unsubscribe {
        id: 3,
        filters: vec![topic.to_owned()],
    });
    let _ = probe.send(&Packet::Disconnect);
    probe.transport.close();
    Ok(())
}
