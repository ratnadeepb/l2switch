/*
 * Copyright 2019 Comcast Cable Communications Management, LLC
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

#include "bindings.h"
#include <rte_errno.h>
#include <rte_ethdev.h>
#include <rte_mbuf.h>
#include <rte_mempool.h>

int
_rte_errno(void)
{
        return rte_errno;
}

struct rte_mbuf *
_rte_pktmbuf_alloc(struct rte_mempool *mp)
{
        return rte_pktmbuf_alloc(mp);
}

void
_rte_pktmbuf_free(struct rte_mbuf *m)
{
        rte_pktmbuf_free(m);
}

int
_rte_pktmbuf_alloc_bulk(struct rte_mempool *pool, struct rte_mbuf **mbufs,
                        unsigned count)
{
        return rte_pktmbuf_alloc_bulk(pool, mbufs, count);
}

void
_rte_mempool_put_bulk(struct rte_mempool *mp, void *const *obj_table,
                      unsigned int n)
{
        rte_mempool_put_bulk(mp, obj_table, n);
}

uint16_t
_rte_eth_rx_burst(uint16_t port_id, uint16_t queue_id,
                  struct rte_mbuf **rx_pkts, const uint16_t nb_pkts)
{
        return rte_eth_rx_burst(port_id, queue_id, rx_pkts, nb_pkts);
}

uint16_t
_rte_eth_tx_burst(uint16_t port_id, uint16_t queue_id,
                  struct rte_mbuf **tx_pkts, uint16_t nb_pkts)
{
        return rte_eth_tx_burst(port_id, queue_id, tx_pkts, nb_pkts);
}

/* Added by Deep */
unsigned int
_rte_ring_count(const struct rte_ring *r)
{
        return rte_ring_count(r);
}

unsigned int
_rte_ring_dequeue_bulk(struct rte_ring *r, void **obj_table, unsigned int n,
                       unsigned int *available)
{
        return rte_ring_dequeue_bulk(r, obj_table, n, available);
}

void
_rte_mempool_put(struct rte_mempool *mp, void *obj)
{
        return rte_mempool_put(mp, obj);
}

int
_rte_mempool_get(struct rte_mempool *mp, void **obj)
{
        return rte_mempool_get(mp, obj);
}

int
_rte_ring_enqueue(struct rte_ring *r, void *obj)
{
        return rte_ring_enqueue(r, obj);
}

uint64_t
_rte_get_tsc_cycles(void)
{
        return rte_get_tsc_cycles();
}

unsigned
_rte_lcore_id(void)
{
        return rte_lcore_id();
}

uint64_t
_rte_get_timer_hz(void)
{
        return rte_get_timer_hz();
}

void
_rte_atomic16_dec(rte_atomic16_t *v)
{
        return rte_atomic16_dec(v);
}

unsigned int
_rte_ring_dequeue_burst(struct rte_ring *r, void **obj_table, unsigned int n,
                        unsigned int *available)
{
        return rte_ring_dequeue_burst(r, obj_table, n, available);
}

int
_rte_ring_dequeue(struct rte_ring *r, void **obj_p)
{
        return rte_ring_dequeue(r, obj_p);
}

unsigned int
_rte_ring_enqueue_bulk(struct rte_ring *r, void *const *obj_table,
                       unsigned int n, unsigned int *free_space)
{
        return rte_ring_enqueue_bulk(r, obj_table, n, free_space);
}

void
stop_and_close_ports()
{
        uint16_t port_id = 0;
        RTE_ETH_FOREACH_DEV(port_id)
        {
                rte_eth_dev_stop(port_id);
                rte_eth_dev_close(port_id);
        }
        exit(0);
}

struct rte_ether_hdr *
_pkt_ether_hdr(struct rte_mbuf *pkt)
{
        if (unlikely(pkt == NULL)) {
                return NULL;
        }
        return rte_pktmbuf_mtod(pkt, struct rte_ether_hdr *);
}

struct rte_ipv4_hdr *
_pkt_ipv4_hdr(struct rte_mbuf *pkt)
{
        struct rte_ipv4_hdr *ipv4 =
            (struct rte_ipv4_hdr *)(rte_pktmbuf_mtod(pkt, uint8_t *) +
                                    sizeof(struct rte_ether_hdr));

        /* In an IP packet, the first 4 bits determine the version.
         * The next 4 bits are called the Internet Header Length, or IHL.
         * DPDK's ipv4_hdr struct combines both the version and the IHL into one
         * uint8_t.
         */
        uint8_t version = (ipv4->version_ihl >> 4) & 0b1111;
        if (unlikely(version != 4)) {
                return NULL;
        }
        return ipv4;
}

#define IP_PROTOCOL_TCP 6
#define IP_PROTOCOL_UDP 17

struct rte_tcp_hdr *
_pkt_tcp_hdr(struct rte_mbuf *pkt)
{
        struct rte_ipv4_hdr *ipv4 = onvm_pkt_ipv4_hdr(pkt);

        if (unlikely(ipv4 ==
                     NULL)) { // Since we aren't dealing with IPv6 packets for
                              // now, we can ignore anything that isn't IPv4
                return NULL;
        }

        if (ipv4->next_proto_id != IP_PROTOCOL_TCP) {
                return NULL;
        }

        uint8_t *pkt_data = rte_pktmbuf_mtod(pkt, uint8_t *) +
                            sizeof(struct rte_ether_hdr) +
                            sizeof(struct rte_ipv4_hdr);
        return (struct rte_tcp_hdr *)pkt_data;
}

struct rte_udp_hdr *
_pkt_udp_hdr(struct rte_mbuf *pkt)
{
        struct rte_ipv4_hdr *ipv4 = onvm_pkt_ipv4_hdr(pkt);

        if (unlikely(ipv4 ==
                     NULL)) { // Since we aren't dealing with IPv6 packets for
                              // now, we can ignore anything that isn't IPv4
                return NULL;
        }

        if (ipv4->next_proto_id != IP_PROTOCOL_UDP) {
                return NULL;
        }

        uint8_t *pkt_data = rte_pktmbuf_mtod(pkt, uint8_t *) +
                            sizeof(struct rte_ether_hdr) +
                            sizeof(struct rte_ipv4_hdr);
        return (struct rte_udp_hdr *)pkt_data;
}