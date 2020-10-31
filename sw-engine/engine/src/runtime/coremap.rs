/*
 * Created on Fri Oct 30 2020:18:07:03
 * Created by Ratnadeep Bhattacharya
 */

use crate::{
	debug,
	dpdk::{CoreId, Mempool, MempoolMap},
	error, ffi, info,
};
use failure::{Fail, Fallible};
use futures;
use std::{
	collections::{HashMap, HashSet},
	sync::mpsc::{self, Receiver, SyncSender},
	thread::{self, JoinHandle},
};

/// A park handle
///
/// Park all cores while initialization completes
pub(crate) struct Park {
	core_id: CoreId,
	sender: SyncSender<()>,
	receiver: Receiver<()>,
}

impl Park {
	fn new(core_id: CoreId) -> Self {
		let (sender, receiver) = mpsc::sync_channel(0);
		Park {
			core_id,
			sender,
			receiver,
		}
	}

	fn unpark(&self) -> Unpark {
		Unpark {
			core_id: self.core_id,
			sender: self.sender.clone(),
		}
	}

	fn park(&self) {
		if let Err(err) = self.receiver.recv() {
			// we are not expecting failures, but we will log it in case.
			error!(message = "park failed.", core=?self.core_id, ?err);
		}
	}
}

/// An unpark handle
///
/// Unpark all cores once initialization is complete
pub(crate) struct Unpark {
	core_id: CoreId,
	sender: SyncSender<()>,
}

impl Unpark {
	pub(crate) fn unpark(&self) {
		if let Err(err) = self.sender.send(()) {
			// we are not expecting failures, but we will log it in case.
			error!(message = "unpark failed.", core=?self.core_id, ?err);
		}
	}
}

/// A futures oneshot channel based shutdown mechanism
pub(crate) struct Shutdown {
	receiver: futures::channel::oneshot::Receiver<()>,
}

/// A abstraction used to interact with the master/main thread
///
/// This is an additional handle to the master thread for performing tasks
/// Use this `thread` handle to run the main loop
/// Use the `reactor` handle to catch Unix signals to terminate the main loop
/// Use the `timer` handle to create new time based tasks with either a `Delay` or `Interval`
pub(crate) struct MasterExecutor {}

/// A thread/core abstraction used to interact with a background thread
/// from the master/main thread
///
/// When a background thread is first spawned, it is parked and waiting for tasks
///
pub(crate) struct CoreExecutor {}
