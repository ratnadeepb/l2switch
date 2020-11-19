/*
 * Created on Wed Nov 18 2020:17:21:42
 * Created by Ratnadeep Bhattacharya
 */

use serde::{Deserialize, Serialize};
use serde_json::Result as sResult;
use std::{
	cell::RefCell,
	fmt, ptr, result,
	sync::{Arc, Mutex},
};
use zmq;

pub const SOCKET: &str = "tcp://localhost:5555";

#[derive(Serialize, Deserialize)]
pub enum MsgType {
	POD_STARTING, // Register with the engine
	POD_READY,    // Let engine know that initialization is complete
	POD_STOPPING, // Pod is ending
}

impl fmt::Display for MsgType {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match &self {
			Self::POD_STARTING => write!(f, "0"),
			Self::POD_READY => write!(f, "1"),
			Self::POD_STOPPING => write!(f, "2"),
		}
	}
}

impl From<MsgType> for zmq::Message {
	fn from(msg: MsgType) -> Self {
		let mut m: u8 = match msg {
			MsgType::POD_STARTING => 0,
			MsgType::POD_READY => 1,
			MsgType::POD_STOPPING => 2,
		};
		let mut data = zmq::Message::with_size(1);
		unsafe { ptr::copy_nonoverlapping(&mut m as *mut _, data.as_mut_ptr(), 1) };
		data
	}
}

// ISSUES: What happens if multiple clients are starting at the same time!
pub static mut CLIENT_ID: u16 = 0;

fn get_dockerclient_id() -> u16 {
	let id: u16;
	unsafe {
		id = CLIENT_ID;
		CLIENT_ID += 1;
	}
	id
}

/// Represents a Docker client
pub struct ContainerClient {
	id: u16,             // id of the client
	socket: zmq::Socket, // socket to send registration, start/stop msgs to the engine
}

type Result<ContainerClient> = result::Result<ContainerClient, zmq::Error>;

impl ContainerClient {
	/// Return a new Client
	pub fn new() -> Result<ContainerClient> {
		let context = zmq::Context::new();
		let id = get_dockerclient_id();
		match context.socket(zmq::REQ) {
			Ok(socket) => Ok(Self { id, socket }),
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
	pub fn send(&self, msg: MsgType) -> zmq::Result<()> {
		let cmsg = match msg {
			MsgType::POD_STARTING => ContainerMsg::registration(self.get_id()).to_json(),
			MsgType::POD_READY => ContainerMsg::ready(self.get_id()).to_json(),
			MsgType::POD_STOPPING => ContainerMsg::terminating(self.get_id()).to_json(),
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
	msg: MsgType,
}

impl ContainerMsg {
	/// Returns a container registration struct
	pub fn registration(id: u16) -> Self {
		Self {
			id,
			msg: MsgType::POD_STARTING,
		}
	}

	/// Returns a container ready struct
	pub fn ready(id: u16) -> Self {
		Self {
			id,
			msg: MsgType::POD_READY,
		}
	}

	/// Returns a container terminating struct
	pub fn terminating(id: u16) -> Self {
		Self {
			id,
			msg: MsgType::POD_STOPPING,
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
