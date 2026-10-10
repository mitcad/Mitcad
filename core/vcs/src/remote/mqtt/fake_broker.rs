// SPDX-License-Identifier: MIT
//! A small MQTT 3.1.1 broker for tests, in this process and over plain TCP:
//! subscriptions with wildcards, retained messages, wills, QoS 0 and 1, and
//! dropping every client at once (a restart). It lets the client's tests
//! run where no broker is installed; `broker_tests.rs` runs the same
//! scenarios against mosquitto when it is.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::MAX_PACKET;
use super::packet::{Decoder, Packet, Publish, Will};

struct Client {
    id: u64,
    stream: TcpStream,
    filters: Vec<String>,
    next_id: u16,
}

#[derive(Default)]
struct State {
    clients: Vec<Client>,
    retained: BTreeMap<String, Vec<u8>>,
    next: u64,
    /// The user and password required, if any.
    credentials: Option<(String, Vec<u8>)>,
}

pub struct FakeBroker {
    port: u16,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
}

/// Whether a topic filter matches a topic name.
pub fn matches(filter: &str, topic: &str) -> bool {
    let mut topic = topic.split('/');
    for level in filter.split('/') {
        match (level, topic.next()) {
            ("#", _) => return true,
            ("+", Some(_)) => {}
            (level, Some(name)) if level == name => {}
            _ => return false,
        }
    }
    topic.next().is_none()
}

impl FakeBroker {
    pub fn start(credentials: Option<(&str, &str)>) -> FakeBroker {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(State {
            credentials: credentials.map(|(u, p)| (u.to_owned(), p.as_bytes().to_vec())),
            ..State::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (s, st) = (Arc::clone(&state), Arc::clone(&stop));
        listener.set_nonblocking(true).unwrap();
        std::thread::spawn(move || {
            while !st.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let s = Arc::clone(&s);
                        std::thread::spawn(move || serve(&s, stream));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        FakeBroker { port, state, stop }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Drops every client without their wills, as a restart does, and
    /// forgets the retained messages (no persistence).
    pub fn restart(&self) {
        let mut state = self.state.lock().unwrap();
        for client in state.clients.drain(..) {
            let _ = client.stream.shutdown(Shutdown::Both);
        }
        state.retained.clear();
    }
}

impl Drop for FakeBroker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.restart();
    }
}

fn send(stream: &mut TcpStream, packet: &Packet) {
    let _ = stream.write_all(&packet.encode().unwrap());
}

/// Sends with the state locked, so that the packets written to a client
/// by its own thread and by others' messages do not interleave.
fn reply(state: &Mutex<State>, stream: &mut TcpStream, packet: &Packet) {
    let _state = state.lock().unwrap();
    send(stream, packet);
}

/// Routes a message to the subscribers and keeps it when retained.
fn route(state: &Mutex<State>, publish: &Publish) {
    let mut state = state.lock().unwrap();
    if publish.retain {
        if publish.payload.is_empty() {
            state.retained.remove(&publish.topic);
        } else {
            state
                .retained
                .insert(publish.topic.clone(), publish.payload.clone());
        }
    }
    for client in &mut state.clients {
        if client.filters.iter().any(|f| matches(f, &publish.topic)) {
            client.next_id = client.next_id.checked_add(1).unwrap_or(1);
            let packet = Packet::Publish(Publish {
                retain: false,
                dup: false,
                id: if publish.qos > 0 { client.next_id } else { 0 },
                ..publish.clone()
            });
            send(&mut client.stream, &packet);
        }
    }
}

fn serve(state: &Arc<Mutex<State>>, mut stream: TcpStream) {
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    // Larger than the client's limit: the tests send it too much.
    let mut decoder = Decoder::new(MAX_PACKET * 4);
    let mut buffer = [0u8; 4096];
    let mut id = None;
    let mut will: Option<Will> = None;
    let mut clean = false;
    'read: loop {
        let n = match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        decoder.push(&buffer[..n]);
        loop {
            let packet = match decoder.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => break,
                Err(_) => break 'read,
            };
            match packet {
                Packet::Connect(connect) => {
                    let mut s = state.lock().unwrap();
                    let accepted = match &s.credentials {
                        None => true,
                        Some((user, password)) => {
                            connect.user.as_ref() == Some(user)
                                && connect.password.as_ref() == Some(password)
                        }
                    };
                    if accepted {
                        s.next += 1;
                        let client_id = s.next;
                        id = Some(client_id);
                        will = connect.will;
                        s.clients.push(Client {
                            id: client_id,
                            stream: stream.try_clone().unwrap(),
                            filters: Vec::new(),
                            next_id: 0,
                        });
                    }
                    let code = if accepted { 0 } else { 4 };
                    let connack = Packet::ConnAck {
                        session_present: false,
                        code,
                    };
                    send(&mut stream, &connack);
                    if !accepted {
                        clean = true;
                        break 'read;
                    }
                }
                Packet::Publish(publish) => {
                    // Kept and routed before the PUBACK, as brokers do: a
                    // client that subscribes after it gets the new state.
                    route(state, &publish);
                    if publish.qos == 1 {
                        reply(state, &mut stream, &Packet::PubAck(publish.id));
                    }
                }
                Packet::Subscribe {
                    id: packet_id,
                    filters,
                } => {
                    let mut s = state.lock().unwrap();
                    let retained: Vec<(String, Vec<u8>)> = s
                        .retained
                        .iter()
                        .filter(|(topic, _)| filters.iter().any(|(f, _)| matches(f, topic)))
                        .map(|(t, p)| (t.clone(), p.clone()))
                        .collect();
                    if let Some(client) = s.clients.iter_mut().find(|c| Some(c.id) == id) {
                        client
                            .filters
                            .extend(filters.iter().map(|(f, _)| f.clone()));
                    }
                    let suback = Packet::SubAck {
                        id: packet_id,
                        codes: filters.iter().map(|(_, q)| (*q).min(1)).collect(),
                    };
                    send(&mut stream, &suback);
                    for (topic, payload) in retained {
                        let publish = Packet::Publish(Publish {
                            topic,
                            payload,
                            qos: 0,
                            retain: true,
                            dup: false,
                            id: 0,
                        });
                        send(&mut stream, &publish);
                    }
                }
                Packet::Unsubscribe {
                    id: packet_id,
                    filters,
                } => {
                    let mut s = state.lock().unwrap();
                    if let Some(client) = s.clients.iter_mut().find(|c| Some(c.id) == id) {
                        client.filters.retain(|f| !filters.contains(f));
                    }
                    send(&mut stream, &Packet::UnsubAck(packet_id));
                }
                Packet::PingReq => reply(state, &mut stream, &Packet::PingResp),
                Packet::Disconnect => {
                    clean = true;
                    break 'read;
                }
                _ => {}
            }
        }
    }
    let mut s = state.lock().unwrap();
    let known = s.clients.iter().any(|c| Some(c.id) == id);
    s.clients.retain(|c| Some(c.id) != id);
    drop(s);
    let _ = stream.shutdown(Shutdown::Both);
    // A client dropped by a restart has no will published.
    if let Some(will) = will
        && !clean
        && known
    {
        route(
            state,
            &Publish {
                topic: will.topic,
                payload: will.payload,
                qos: will.qos,
                retain: will.retain,
                dup: false,
                id: 0,
            },
        );
    }
}
