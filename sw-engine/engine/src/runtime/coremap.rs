/*
 * Created on Fri Oct 30 2020:18:07:03
 * Created by Ratnadeep Bhattacharya
 */

use crate::{
	debug,
	dpdk::{CoreId, Mempool, MempoolMap, PortQueue, MEMPOOL, Mbuf},
	error, ffi, info,
	stealers
};
use failure::{Fail, Fallible};
use futures;
use std::{
	collections::{HashMap, HashSet},
	sync::mpsc::{self, Receiver, SyncSender},
	thread::{self, JoinHandle},
	cell::Cell, ptr,
	sync::{Arc, Mutex}
};
use crossbeam_deque::{Injector, Steal, Worker};

/// RX or TX worker
pub(crate) enum WorkerType {
	RX, // RX task
	TX, // TX task
}

/// Task type
pub(crate) enum ExecutorType {
	Master(fn()), // Provide the management function
	Worker(WorkerType),
}

/// An executor is really a task
/// Master task is responsible for running the management of the server
/// An RX taskk reads packets
/// A TX task sends packets out
pub(crate) struct Executor {
	pub(crate) builder: thread::Builder, // A thread can be spawned from the builder
	pub(crate) core_id: CoreId, // The core to which this thread should affinitise
	// pub(crate) worker: futures::executor::LocalSpawner,
	pub(crate) task_type: ExecutorType,
}

/// Core Errors
#[derive(Debug, Fail)]
pub(crate) enum CoreError {
	/// Core is not found
	#[fail(display = "{:?} is not found", _0)]
	NotFound(CoreId),

	/// Core is not assigned to any port
	#[fail(display = "{:?} is not assigned to any port", _0)]
	NotAssigned(CoreId),
}

/// Map all core handles
pub(crate) struct CoreMap {
	// pub(crate) master_core: Executor,
	pub(crate) cores: HashMap<CoreId, Executor>,
}

struct SendableMemPtr(*mut dpdk_ffi::rte_mempool);
unsafe impl std::marker::Send for SendableMemPtr {}

/// Builder for Core Map
pub(crate) struct CoreMapBuilder<'a> {
	cores: HashSet<CoreId>,
	master_core: CoreId,
	mempools: MempoolMap<'a>,
}

impl<'a> CoreMapBuilder<'a> {
	pub(crate) fn cores(&mut self, cores: &[CoreId]) -> &mut Self {
		self.cores = cores.iter().cloned().collect();
		self
	}
	pub(crate) fn master_core(&mut self, master_core: CoreId) -> &mut Self {
		self.master_core = master_core;
		self
	}
	pub(crate) fn mempools(&'a mut self, mempools: &'a mut [Mempool]) -> &'a mut Self {
		self.mempools = MempoolMap::new(mempools);
		self
	}
	#[allow(clippy::cognitive_complexity)]
	pub(crate) fn finish(&'a mut self) -> Fallible<CoreMap> {
		let mut map = HashMap::new();

		// initialise master core
		// and affinitise thread
		let socket_id = self.master_core.socket_id();
		let mempool = self.mempools.get_raw(socket_id)?;

		let master_executor =
			init_core(self.master_core, mempool, ExecutorType::Master(main_loop)).unwrap();

		// add master core to the map
		map.insert(self.master_core, master_executor);

		info!("Initialized master core on {:?}", self.master_core);

		// remove from core list, if it exists, to avoid double init
		self.cores.remove(&self.master_core);

		// initialise all other cores
		for &core_id in self.cores.iter() {
			// find the mempool that matches the core's socket
			// wrap it up as a SendableMemPtr
			let socket_id = core_id.socket_id();
			let mempool = self.mempools.get_raw(socket_id)?;
			let ptr = SendableMemPtr(mempool);

			match init_core(core_id, ptr.0, ExecutorType::Worker(core_id))
		}

		Ok(CoreMap {
			// master_core: master_executor,
			cores: map,
		})
	}
}

/// Create a global injector queue for transmitting packets
/// Must be run in the master core before starting the trasnmitter task
fn create_global_tx_q() -> Injector<Mbuf> {
	let global_tx_q: Injector<Mbuf> = Injector::new();
	global_tx_q
}

/// Create worker and an associated stealer
/// Add the stealer to the global stealer vector
/// Run by each transmit task from each "core" thread
fn create_and_add_stealer() -> Worker<Mbuf> {
	let worker: Worker<Mbuf> = Worker::new_fifo();
	let s = worker.stealer();
	stealers.borrow_mut().push(s);
	worker
}

/// Find packets in a batch of 32 to transmit
/// Run by each transmit task on each "core" thread
fn find_packets_batch_to_transmit(local: &Worker<Mbuf>, global: &Injector<Mbuf>) -> Vec<Mbuf> {
	// vector to hold all the packets that needs to be sent
	let pkts: Vec<Mbuf> = Vec::with_capacity(32);

	// if number of packets in local queue is less than 32
	if local.len() < 32 {
		// try and steal a batch of work from the global queue
		// the max size defined in the crossbeam crate is 32 as of now
		// but it is an internal value and can change
		global.steal_batch(local);
	}

	// is the local queue empty
	let empty = local.len() == 0;
	// Set batch size to total number of packets in the local queue
	// or 32 if the local queue has more packets
	let batch_size = if !empty && local.len() > 32 {
		local.len()
	} else {
		32_usize
	};

	// the local queue still does not have 32 packets
	// NOTE: This is where a scheduling algorithm can help
	// How many packets should be stolen from each queue???
	// Size of the local queue in other threads cannot be obtained from the stealer
	// REVIEW: For now the task will use round robin to go through each stealer exactly once
	// trying to steal a packet from each one of them
	// NOTE: Issues being ignored
	// 1. There are no guarantees that we will get 32 packets because we are going through the stealers only once. If we iterate through the stealers multiple times to try and find 32 packets, we are adding latency.
	// 		I.  How long will it take to find 32 packets?
	//		II. Should we ignore the queues that are empty now? They might have packets later, if a multiple round iteration goes on for some time (if packets are accumulating slowly).
	// 2. Stealing a packet from another thread moves a Mbuf ownership from one thread to another. This is additional work. Is this work justified? But maybe inconsequential.
	// 3. If packets are trickling in slowly and every queue is trying to steal from the other queues, then what is the impact of that competition on our performance, especially if we use multiple rounds of iterations?
	if local.len() < 32 {
		for s in stealers.borrow().iter() {
			match s.steal() {
				Steal::Success(p) => local.push(p), // Get a packet if found
				_ => {}, // ignore other cases
			}
		}

	}
	
	for _ in 0..batch_size {
		match local.pop() {
			Some(p) => pkts.push(p), // add packets to the output
			None => break, // break; no more packets
		}
	}
	pkts
}

fn init_core(
	id: CoreId,
	mempool: *mut dpdk_ffi::rte_mempool,
	ex: ExecutorType,
) -> Fallible<Executor> {
	// affinitise thread to core
	// id.set_thread_affinity()?;

	// set the mempool
	// MEMPOOL.with(|tls| tls.set(mempool));

	// let task_pool = futures::executor::LocalPool::new();
	match ex {
		ExecutorType::Master(f) => Ok(Executor {
			// worker: task_pool.spawner(),
			builder: thread::Builder::new().name("Master_Thread".into()),
			core_id: id,
			task_type: ExecutorType::Master(f),
		}),
		ExecutorType::Worker(portqueue) => Ok(Executor {
			builder: thread::Builder::new().name("Worker_Thread".into()),
			core_id: id,
			task_type: ExecutorType::Worker(portqueue),
		}),
	}
}

fn main_loop() {
	println!("Main loop placeholder");
}
