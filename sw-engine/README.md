# DPDK-based Virtual Switch for Container Networking
## Overview
The core is made up of a L2 forwarding table that receives and send packets. The engine is designed as a bag of asynchronous tasks:
- get packets from NICs
- process incoming packets - get destination mac and IP and add them to the forwarding table; send the packets to the recipient pod through a DPDK ring
- look out for new clients starting up

The core would implement Layer 2 protocols like IP and ARP. It would also implement Layer 2 level security policies.

The second part of the switch would be implemented inside a proxy-like sidecar container which would connect directly to the other end of the DPDK ring. This sidecar would implement Layer 3 protocols like TCP and UDP along with Layer 3 security policies.

The design is heavily influenced by [openNetVM](https://github.com/sdnfv/openNetVM) and [capsule](https://github.com/capsule-rs/capsule).

## Implementation Notes
Implementing the core engine as a "bag of asynchronous task" has the advantage that even one core can be utilized for running all the tasks. This is done by:
- implementing the tasks as futures and polling them regularly
- there is synchronization between receiving and transmitting packets:
	- packets are stored in a concurrent queue (crossbeam ArrayQueue)
	- packets are received and added to one end of the array till the queue has space
	- packets are removed from the other end of the queue till the queue is not empty
	- if the queue is empty, then the future representing packet removal waits allowing the future representing packet addition to run
- New clients send a message over a message queue (zeromq in this case) to the engine when starting allowing the engine to add them to the list of known clients.