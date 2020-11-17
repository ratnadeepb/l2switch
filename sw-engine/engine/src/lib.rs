/*
 * Created on Wed Oct 28 2020:13:50:19
 * Created by Ratnadeep Bhattacharya
 */

// alias for macros
// extern crate self as engine;

pub mod config;
mod dpdk;
mod ffi;
mod macros;
// pub mod metrics;
pub mod net;
mod runtime;

use crate::dpdk::{Mbuf, PortQueue};
// use crossbeam_deque::Stealer;
// use lazy_static::lazy_static;
use crate::net::{FiveTuple, RoutingTable};
use dashmap::DashMap;
use state;
use std::{
    cell::Cell,
    collections::{hash_map::RandomState, HashMap},
    mem,
};

pub const PACKET_READ_SIZE: usize = 32;
pub const ETHER_HDR_SZ: usize = mem::size_of::<dpdk_ffi::rte_ether_hdr>();
pub const IPV4_HDR_SZ: usize = mem::size_of::<dpdk_ffi::rte_ipv4_hdr>();

// pub static STEALERS: Arc<RefCell<Vec<Stealer<Mbuf>>>> = Arc::new(RefCell::new(Vec::new()));
// lazy_static! {
//     pub static ref PORTS: Arc<Vec<PortQueue>> = Arc::new(Vec::new());
// }

pub static PORTS: state::Storage<Vec<PortQueue>> = state::Storage::new();
// NOTE: Careful with a DashMap.
// We will only use the insert, clear and maybe remove functionalities here
// It allows multithreaded support without and explicit RWLock
// However, can deadlock in case there are any references held to any object inside
pub static PORTMAP: state::Storage<DashMap<u16, FiveTuple, RandomState>> = state::Storage::new();
pub static FORWARDING_TABLE: state::Storage<RoutingTable> = state::Storage::new();

thread_local! {
    pub static MBUFS_RECVD: Cell<HashMap<u16, Vec<Mbuf>>> = Cell::new(HashMap::new());
    // pub static MBUFS_RECVD: Cell<Vec<Mbuf>> = Cell::new(Vec::with_capacity(PACKET_READ_SIZE));
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
