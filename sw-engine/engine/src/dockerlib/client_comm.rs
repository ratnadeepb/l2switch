/*
 * Created on Wed Nov 18 2020:17:21:42
 * Created by Ratnadeep Bhattacharya
 */

use crate::dpdk::{Channel, Ring, RingType};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
// use serde_json::Result as sResult;
use std::{
	cell::RefCell,
	fmt, ptr, result,
	sync::{Arc, RwLock},
};
use zmq;

pub const SOCKET: &str = "tcp://localhost:5555";

#[derive(Serialize, Deserialize)]
pub enum ContMsgType {
	PodStarting, // Register with the engine
	PodReady,    // Let engine know that initialization is complete
	PodStopping, // Pod is ending
}

impl fmt::Display for ContMsgType {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match &self {
			Self::PodStarting => write!(f, "0"),
			Self::PodReady => write!(f, "1"),
			Self::PodStopping => write!(f, "2"),
		}
	}
}

impl From<ContMsgType> for zmq::Message {
	fn from(msg: ContMsgType) -> Self {
		let mut m: u8 = match msg {
			ContMsgType::PodStarting => 0,
			ContMsgType::PodReady => 1,
			ContMsgType::PodStopping => 2,
		};
		let mut data = zmq::Message::with_size(1);
		unsafe { ptr::copy_nonoverlapping(&mut m as *mut _, data.as_mut_ptr(), 1) };
		data
	}
}

lazy_static! {
	pub static ref CLIENT_ID: RwLock<u16> = RwLock::new(0);
}

fn get_dockerclient_id() -> u16 {
	let mut id = 0;
	if let Ok(mut cl_id) = CLIENT_ID.write() {
		id = *cl_id;
		*cl_id += 1;
	}
	id
}

/// Represents a Docker client
pub struct ContainerClient {
	id: u16,                  // id of the client
	socket: zmq::Socket,      // socket to send registration, start/stop msgs to the engine
	channel: Option<Channel>, // channel to communicate with engine
}

type Result<ContainerClient> = result::Result<ContainerClient, zmq::Error>;

impl ContainerClient {
	/// Return a new Client
	pub fn new() -> Result<ContainerClient> {
		let context = zmq::Context::new();
		let id = get_dockerclient_id();
		match context.socket(zmq::REQ) {
			Ok(socket) => Ok(Self {
				id,
				socket,
				channel: None,
			}),
			Err(err) => Err(err),
		}
	}

	/// Get client ID
	pub fn get_id(&self) -> u16 {
		self.id
	}

	/// Get client's channel to engine
	pub fn get_socket(&self) -> &zmq::Socket {
		&self.socket
	}

	/// Send message to engine about container state
	/// In case of failures it will attempt thrice to connect
	/// and thrice to send, in case of a successful connection
	pub fn send(&self, msg: ContMsgType) -> zmq::Result<()> {
		let cmsg = match msg {
			ContMsgType::PodStarting => ContainerMsg::registration(self.get_id()).to_json(),
			ContMsgType::PodReady => ContainerMsg::ready(self.get_id()).to_json(),
			ContMsgType::PodStopping => ContainerMsg::terminating(self.get_id()).to_json(),
		};

		if let Err(ce) = self.get_socket().connect(SOCKET) {
			let mut connected = false;
			// retry twice to send the message before sending back the error
			for _ in 0..2 {
				if let Ok(_) = self.get_socket().connect(SOCKET) {
					connected = true;
					break;
				}
			}
			if !connected {
				return Err(ce); // failed to connect to socket after three attempts
			}
		} else {
			// connection successful
			if let Err(se) = self.get_socket().send(&cmsg, 0) {
				let mut sent = false;
				for _ in 0..2 {
					if let Ok(_) = self.get_socket().send(&cmsg, 0) {
						sent = true;
						break;
					}
				}
				if !sent {
					return Err(se); // failed to send message after three attempts
				}
			}
		}
		Ok(()) // successfully sent
	}

	/// Register with the engine at startup
	pub fn register(&self) -> zmq::Result<()> {
		match self.send(ContMsgType::PodStarting) {
			Err(e) => return Err(e),
			Ok(_) => {
				let mut msg = zmq::Message::new();
				self.get_socket().recv(&mut msg, 0)
			}
		}
	}

	/// Notify the engine that the container is ready
	pub fn notify(&self) -> zmq::Result<()> {
		self.send(ContMsgType::PodReady)
	}

	/// Notify the engine that the container is terminating
	pub fn terminate(&self) -> zmq::Result<()> {
		self.send(ContMsgType::PodStopping)
	}

	/// Once message from engine is successfully received
	/// look up the tx and rx channels and set it up as
	/// self.tx_channel and self.rx_channel
	pub fn setup_channel(&mut self) {
		if let Ok(_) = self.register() {
			if let Some(t_ring) = Ring::from_ptr(self.id, RingType::TX, unsafe {
				dpdk_ffi::rte_ring_lookup(format!("TX-{}", self.id).as_ptr() as *const _)
			}) {
				if let Some(r_ring) = Ring::from_ptr(self.id, RingType::RX, unsafe {
					dpdk_ffi::rte_ring_lookup(format!("RX-{}", self.id).as_ptr() as *const _)
				}) {
					self.channel = Some(Channel {
						tx_q: t_ring,
						rx_q: r_ring,
					})
				}
			}
		}
	}
}

/// Represents a message to be sent to the engine
///
/// The message is sent in the following format for a container event
///
/// let msg = r#"
///        {
///            'id': 1,
///            'msg': 1,
///        }"#,
///
/// This corresponds to the message that container 1 is ready to run
#[derive(Serialize, Deserialize)]
pub struct ContainerMsg {
	id: u16,
	msg: ContMsgType,
}

impl ContainerMsg {
	/// Returns a container registration struct
	pub fn registration(id: u16) -> Self {
		Self {
			id,
			msg: ContMsgType::PodStarting,
		}
	}

	/// Returns a container ready struct
	pub fn ready(id: u16) -> Self {
		Self {
			id,
			msg: ContMsgType::PodReady,
		}
	}

	/// Returns a container terminating struct
	pub fn terminating(id: u16) -> Self {
		Self {
			id,
			msg: ContMsgType::PodStopping,
		}
	}

	pub fn to_json(&self) -> String {
		format!(
			r#"
        {{
            'id': {},
            'msg': {},
        }}"#,
			self.id, self.msg
		)
	}
}
