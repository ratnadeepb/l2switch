/*
 * Created on Tue Nov 17 2020:03:16:30
 * Created by Ratnadeep Bhattacharya
 */

use super::mac::MacAddr;
use crossbeam_utils::sync::ShardedLock; // faster reads than RWLock but slower writes
use std::{collections::HashMap, net::Ipv4Addr, sync::Arc};

/// Here we try to build a routing table that contains a mapping between a MAC and IP of containers running on the server
/// This table is populated only for response packets
pub struct RoutingTable {
	// An alternate approach would be to have a global routing table protected by
	// a ShardedLock. A single write access to that lock would enable updating both tables
	// Taking separate, concurrent locks on two tables is more synchronisation overhead
	mac_table: Arc<ShardedLock<HashMap<MacAddr, Ipv4Addr>>>,
	ip_table: Arc<ShardedLock<HashMap<Ipv4Addr, MacAddr>>>,
}

impl RoutingTable {
	/// Add a mac and ip to the routing table
	pub fn add(&self, mac: MacAddr, ip: Ipv4Addr) {
		// NOTE: This is the only function that needs a write lock.
		// The assumption is that it runs much less than the read functions

		// take the lock for both tables or neither of them
		if let Ok(mut mac_table) = self.mac_table.write() {
			if let Ok(mut ip_table) = self.ip_table.write() {
				// if either of the tables doesn't contain the ip or the mac
				// we update both the tables
				if !self.contains_mac(&mac) || !self.contains_ip(&ip) {
					(*mac_table).insert(mac, ip);
					(*ip_table).insert(ip, mac);
				}
			}
		}
	}

	/// Check if the MAC is registered for any of the containers
	pub fn contains_mac(&self, mac: &MacAddr) -> bool {
		if let Ok(table) = (*self.mac_table).read() {
			(*table).contains_key(mac)
		} else {
			// ISSUES: Need better handling of PANIC here
			false
		}
	}

	/// Check if the IP is registered for any of the containers
	pub fn contains_ip(&self, ip: &Ipv4Addr) -> bool {
		if let Ok(table) = (*self.ip_table).read() {
			(*table).contains_key(ip)
		} else {
			// ISSUES: Need better handling of PANIC here
			false
		}
	}

	/// Returns an Option<&MacAddr>
	pub fn get_mac(&self, ip: &Ipv4Addr) -> Option<MacAddr> {
		// ISSUES: Need better handling of PANIC here
		match (*self.ip_table).read() {
			Ok(table) => match (*table).get(ip) {
				Some(mac) => Some(*mac),
				_ => None, // Didn't get MAC
			},
			_ => None, // Couldn't get read lock on tables
		}
	}

	/// Returns an Option<&Ipv4Addr>
	pub fn get_ip(&self, mac: &MacAddr) -> Option<Ipv4Addr> {
		// ISSUES: Need better handling of PANIC here
		match (*self.mac_table).read() {
			Ok(table) => match (*table).get(mac) {
				Some(ip) => Some(*ip),
				_ => None,
			},
			_ => None,
		}
	}
}
