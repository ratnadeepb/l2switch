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
pub mod dockerlib;
pub mod net;
pub mod runtime;

use crate::dpdk::{Mbuf, PortQueue};
use crate::net::{FiveTuple, PortIdMbuf, RoutingTable};
use dashmap::DashMap;
use state;
use std::collections::hash_map::RandomState;

pub const PACKET_READ_SIZE: usize = 32;

pub static PORTS: state::Storage<Vec<PortQueue>> = state::Storage::new();

// NOTE: Careful with a DashMap.
// We will only use the insert, clear and maybe remove functionalities here
// It allows multithreaded support without and explicit RWLock
// However, can deadlock in case there are any references held to any object inside
pub static PORTMAP: state::Storage<DashMap<u16, FiveTuple, RandomState>> = state::Storage::new();

pub static FORWARDING_TABLE: state::Storage<RoutingTable> = state::Storage::new();

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
