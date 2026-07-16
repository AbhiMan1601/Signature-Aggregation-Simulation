# Ethereum Fast-Finality Aggregation Experiments

We ran a Dockerized Monte Carlo sensitivity experiment for Ethereum fast-finality aggregation. The experiment compares single-slot finality (SSF) and three-slot finality (3SF) under different network and aggregation topologies. We also added a deterministic Ethereum node-level latency and bandwidth experiment that applies mainnet slot timing, beacon-chain GossipSub parameters, and citable node bandwidth profiles to signature aggregation. This code has been modified after being forked from the research paper codebase hosted at https://anonymous.4open.science/r/Wonderboom-ethereum and has been used to produce these results.

The key result is:

> 3SF reduces per-slot aggregation pressure relative to SSF, but neither SSF nor 3SF is viable at million-validator scale over flat all-validator GossipSub in this model. Fast finality becomes viable only when the aggregation topology is hierarchical or committee-based.


This repository contains work on how to simulate and study signature aggregation:

1. `tree` (the default binary): the original async TCP simulator. It runs one
   measured validator/aggregator path and simulates the surrounding peers.
2. `compare_topologies`: a fast analytical runner for standalone single-slot
   finality (SSF) and three-slot finality (3SF) sweeps over validator counts and
   network topology assumptions.
   
The relevant engineering question is:

> Can signature aggregation round or rounds fit inside the slot budget?

We frame this using two complementary models:

1. An information-theoretic lower bound determined by vote information size and bounded bandwidth.
2. A CTMC/topology model that captures GossipSub mesh degree, effective parallelism, topology, aggregation parallelism, congestion, and duplicate forwarding.

The updated simulator follows that structure: every topology is constrained by an evidence-transfer floor, and topology-specific costs are added through propagation, congestion, crypto, and collection terms.

## Monte Carlo Simulations Using Randomized Parameter Ranges

Each Monte Carlo trial samples valid values from bounded ranges:

| Parameter | Range | Purpose |
|---|---:|---|
| quorum | 0.667 to 0.90 | fraction of validator keys needed |
| bandwidth | 50 to 500 Mbps | available receive/aggregation bandwidth |
| evidence bytes per validator | 1 to 16 bytes | compressed per-validator evidence/accounting |
| raw vote bytes | 96 to 160 bytes | raw vote/signature message size |
| hop latency | 150 to 900 ms | network hop delay |
| duplicate factor | 1.0 to 2.5 | congestion and duplicate forwarding |
| crypto multiplier | 0.75 to 1.75 | machine/implementation variation |
| cores | 4 to 16 | CPU parallelism |

Run the default sweep:

```sh
cargo run --release --bin compare_topologies
```

Export CSV:

```sh
cargo run --release --bin compare_topologies -- --csv
```

Choose validator counts:

```sh
cargo run --release --bin compare_topologies -- --validators 128000,512000,1000000,2097152 --csv
```

Choose Monte Carlo trials and seed:

```sh
cargo run --release --bin compare_topologies -- --trials 2000 --seed 42 --csv
```

The output columns mean:

- `p50_round_ms`, `p90_round_ms`, `p99_round_ms`: Monte Carlo percentiles for
  one full-validator aggregation round.
- `ssf_p50_slot_ms`: median SSF slot aggregation time, modeled as two vote
  aggregation rounds in the same slot.
- `three_sf_p50_slot_ms`: median 3SF slot aggregation time, modeled as one vote
  aggregation round per slot.
- `three_sf_p50_finality_ms`: median 3SF finality aggregation time across three
  slots.
- `ssf_fit_12s_pct`, `three_sf_fit_12s_pct`: percentage of randomized trials
  fitting within a 12 second slot.

The topology models are intentionally simple:

- `evidence-lower-bound`: compressed evidence transfer floor.
- `gossipsub-conservative`: flat GossipSub-style propagation with lower
  effective parallelism.
- `gossipsub-optimized`: flat GossipSub-style propagation with higher effective
  parallelism.
- `committee-hierarchical`: generic local committee aggregation followed by
  higher-level aggregation.
- `central-collector`: all quorum votes delivered to one aggregator.
- `sequential-chain`: sequential forwarding, included as a worst-case sanity
  check.

The experiment ran 2,000 trials with seed `42` and compared SSF and 3SF under
different aggregation topologies. The main result is that 3SF reduces per-slot
aggregation pressure relative to SSF, but flat all-validator GossipSub is still
too slow at million-validator scale in this model. Fast finality only fits the
12 second slot budget when aggregation is hierarchical or committee-based. See
the sources above for the original protocol framing, Wonderboom simulator
context, and topology-model motivation.

Representative median results:

| Validators | Topology | p50 round | SSF p50 slot | 3SF p50 slot | SSF fit 12s | 3SF fit 12s |
|---:|---|---:|---:|---:|---:|---:|
| 1,000,000 | evidence-lower-bound | 194.1 ms | 388.1 ms | 194.1 ms | 100.0% | 100.0% |
| 1,000,000 | gossipsub-conservative | 212.2 s | 424.4 s | 212.2 s | 0.0% | 0.0% |
| 1,000,000 | gossipsub-optimized | 94.1 s | 188.3 s | 94.1 s | 0.0% | 0.0% |
| 1,000,000 | committee-hierarchical | 1.84 s | 3.68 s | 1.84 s | 99.9% | 100.0% |
| 1,000,000 | central-collector | 90.8 s | 181.6 s | 90.8 s | 0.0% | 0.0% |
| 2,500,000 | evidence-lower-bound | 475.2 ms | 950.4 ms | 475.2 ms | 100.0% | 100.0% |
| 2,500,000 | gossipsub-conservative | 529.4 s | 1,058.7 s | 529.4 s | 0.0% | 0.0% |
| 2,500,000 | gossipsub-optimized | 236.1 s | 472.3 s | 236.1 s | 0.0% | 0.0% |
| 2,500,000 | committee-hierarchical | 2.24 s | 4.47 s | 2.24 s | 99.2% | 100.0% |
| 2,500,000 | central-collector | 221.6 s | 443.2 s | 221.6 s | 0.0% | 0.0% |

### Experiment Using Real Ethereum Node Parameters

The new runner is:

```sh
cargo run --release --bin ethereum_node_latency -- --csv
```

This one fixes Ethereum node and protocol constraints from public references and evaluates the same SSF/3SF timing question under those constraints.

The node experiment uses public Ethereum sources:

- The Ethereum consensus specs mainnet configuration defines a 12 second slot.
- The Ethereum consensus P2P interface defines the GossipSub mesh target `D=8`, heartbeat interval `0.7s`, maximum gossip clock disparity `500ms`, 64 attestation subnets, and 2 long-lived subnets per node.
- The honest validator spec defines attestation publication at `SECONDS_PER_SLOT / INTERVALS_PER_SLOT` and aggregate publication at `2 * SECONDS_PER_SLOT / INTERVALS_PER_SLOT`, which correspond to 4 seconds and 8 seconds under mainnet timing.
- ethereum.org's run-a-node guide lists 10+ MBit/s as minimum bandwidth and 25+ MBit/s as recommended bandwidth.
- Cortes-Goicoechea et al., "Can we run our Ethereum nodes at home?", arXiv:2311.05252, provide the 100 Mbit/s Raspberry Pi, 250 Mbit/s default node, and 10,000 Mbit/s fat-node control-machine profiles used in the deterministic sweep.

Public inputs:

| Input | Value used | Source |
|---|---:|---|
| slot time | 12 s | Ethereum consensus specs mainnet config |
| attestation publish time | 4 s into slot | Honest validator spec: `SECONDS_PER_SLOT / INTERVALS_PER_SLOT`; mainnet uses 12 s and `INTERVALS_PER_SLOT=3` |
| aggregate publish time | 8 s into slot | Honest validator spec: `2 * SECONDS_PER_SLOT / INTERVALS_PER_SLOT` |
| GossipSub mesh target `D` | 8 | Ethereum consensus P2P interface |
| GossipSub heartbeat | 0.7 s | Ethereum consensus P2P interface |
| max gossip clock disparity | 500 ms | Ethereum consensus P2P interface |
| attestation subnets | 64 | Ethereum consensus P2P interface |
| minimum node bandwidth | 10 Mbit/s | ethereum.org run-a-node guide |
| recommended node bandwidth | 25 Mbit/s | ethereum.org run-a-node guide |
| measured control machines | 100, 250, and 10,000 Mbit/s | Cortes-Goicoechea et al., "Can we run our Ethereum nodes at home?", arXiv:2311.05252 |

The runner tests two scenarios:

1. `ethereum-committee-subnets`: current Ethereum-style committee/subnet aggregation. It uses 32 slots per epoch, up to 64 committees per slot, target committee size 128, max committee size 2,048, and 16 target aggregators per committee.
2. `global-quorum-collector`: a stress case where one node must collect quorum votes from the whole validator set. This is intentionally harsh and represents the bottleneck a naive all-validator SSF/3SF aggregation design would face.

The timing equation is:

```text
local_aggregation_ms = max(local_receive_ms, local_crypto_ms, local_gossip_ms)
global_collection_ms = max(global_receive_ms, global_crypto_ms, global_gossip_ms)
round_ms             = local_aggregation_ms + global_collection_ms
```

For the global-quorum collector, the whole quorum collection is treated as one round:

```text
round_ms = max(raw_receive_ms, crypto_ms, gossip_ms)
```

The same protocol mapping is then applied:

```text
SSF_slot     = 2 * round_ms
3SF_slot     = round_ms
3SF_finality = 3 * round_ms
```

## Results

### Full Table Summary

| Validators | Topology | p50 round | p90 round | p99 round | SSF p50 slot | 3SF p50 slot | 3SF p50 finality | SSF fit | 3SF fit |
|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 128,000 | evidence-lower-bound | 24.7 ms | 80.3 ms | 168.5 ms | 49.5 ms | 24.7 ms | 74.2 ms | 100.0% | 100.0% |
| 128,000 | gossipsub-conservative | 27.4 s | 39.1 s | 48.6 s | 54.8 s | 27.4 s | 82.2 s | 0.0% | 0.0% |
| 128,000 | gossipsub-optimized | 12.3 s | 18.4 s | 24.0 s | 24.5 s | 12.3 s | 36.8 s | 0.7% | 47.5% |
| 128,000 | committee-hierarchical | 1.64 s | 3.13 s | 4.03 s | 3.29 s | 1.64 s | 4.93 s | 100.0% | 100.0% |
| 128,000 | central-collector | 12.1 s | 23.4 s | 33.2 s | 24.3 s | 12.1 s | 36.4 s | 2.9% | 49.1% |
| 128,000 | sequential-chain | 51,800.6 s | 82,992.9 s | 97,873.0 s | 103,601.2 s | 51,800.6 s | 155,401.8 s | 0.0% | 0.0% |
| 256,000 | evidence-lower-bound | 49.3 ms | 143.7 ms | 344.1 ms | 98.6 ms | 49.3 ms | 147.9 ms | 100.0% | 100.0% |
| 256,000 | gossipsub-conservative | 54.9 s | 79.1 s | 97.6 s | 109.7 s | 54.9 s | 164.6 s | 0.0% | 0.0% |
| 256,000 | gossipsub-optimized | 24.5 s | 37.9 s | 47.9 s | 48.9 s | 24.5 s | 73.4 s | 0.0% | 0.9% |
| 256,000 | committee-hierarchical | 1.78 s | 3.18 s | 4.24 s | 3.57 s | 1.78 s | 5.35 s | 99.9% | 100.0% |
| 256,000 | central-collector | 23.9 s | 45.0 s | 66.4 s | 47.9 s | 23.9 s | 71.8 s | 0.0% | 3.4% |
| 256,000 | sequential-chain | 105,823.6 s | 166,025.6 s | 194,552.3 s | 211,647.3 s | 105,823.6 s | 317,470.9 s | 0.0% | 0.0% |
| 512,000 | evidence-lower-bound | 97.7 ms | 304.3 ms | 719.0 ms | 195.5 ms | 97.7 ms | 293.2 ms | 100.0% | 100.0% |
| 512,000 | gossipsub-conservative | 108.9 s | 159.8 s | 194.3 s | 217.7 s | 108.9 s | 326.6 s | 0.0% | 0.0% |
| 512,000 | gossipsub-optimized | 49.0 s | 73.9 s | 99.6 s | 98.1 s | 49.0 s | 147.1 s | 0.0% | 0.0% |
| 512,000 | committee-hierarchical | 1.82 s | 3.39 s | 4.55 s | 3.64 s | 1.82 s | 5.47 s | 99.9% | 100.0% |
| 512,000 | central-collector | 47.7 s | 88.9 s | 137.2 s | 95.3 s | 47.7 s | 143.0 s | 0.0% | 0.0% |
| 512,000 | sequential-chain | 209,055.4 s | 333,177.3 s | 384,488.8 s | 418,110.8 s | 209,055.4 s | 627,166.2 s | 0.0% | 0.0% |
| 1,000,000 | evidence-lower-bound | 194.1 ms | 611.8 ms | 1.34 s | 388.1 ms | 194.1 ms | 582.2 ms | 100.0% | 100.0% |
| 1,000,000 | gossipsub-conservative | 212.2 s | 304.3 s | 380.4 s | 424.4 s | 212.2 s | 636.6 s | 0.0% | 0.0% |
| 1,000,000 | gossipsub-optimized | 94.1 s | 145.7 s | 186.6 s | 188.3 s | 94.1 s | 282.4 s | 0.0% | 0.0% |
| 1,000,000 | committee-hierarchical | 1.84 s | 3.55 s | 5.24 s | 3.68 s | 1.84 s | 5.52 s | 99.9% | 100.0% |
| 1,000,000 | central-collector | 90.8 s | 175.5 s | 252.5 s | 181.6 s | 90.8 s | 272.4 s | 0.0% | 0.0% |
| 1,000,000 | sequential-chain | 390,512.8 s | 633,718.5 s | 739,602.9 s | 781,025.5 s | 390,512.8 s | 1,171,538.3 s | 0.0% | 0.0% |
| 2,097,152 | evidence-lower-bound | 411.0 ms | 1.26 s | 2.71 s | 821.9 ms | 411.0 ms | 1.23 s | 100.0% | 100.0% |
| 2,097,152 | gossipsub-conservative | 450.1 s | 645.9 s | 810.1 s | 900.2 s | 450.1 s | 1,350.3 s | 0.0% | 0.0% |
| 2,097,152 | gossipsub-optimized | 201.0 s | 302.4 s | 393.6 s | 402.0 s | 201.0 s | 603.0 s | 0.0% | 0.0% |
| 2,097,152 | committee-hierarchical | 2.15 s | 4.14 s | 5.91 s | 4.29 s | 2.15 s | 6.44 s | 99.2% | 100.0% |
| 2,097,152 | central-collector | 191.8 s | 370.4 s | 556.6 s | 383.6 s | 191.8 s | 575.5 s | 0.0% | 0.0% |
| 2,097,152 | sequential-chain | 852,863.5 s | 1,345,007.7 s | 1,586,659.3 s | 1,705,726.9 s | 852,863.5 s | 2,558,590.4 s | 0.0% | 0.0% |
| 2,500,000 | evidence-lower-bound | 475.2 ms | 1.49 s | 3.38 s | 950.4 ms | 475.2 ms | 1.43 s | 100.0% | 100.0% |
| 2,500,000 | gossipsub-conservative | 529.4 s | 764.0 s | 968.4 s | 1,058.7 s | 529.4 s | 1,588.1 s | 0.0% | 0.0% |
| 2,500,000 | gossipsub-optimized | 236.1 s | 358.7 s | 461.4 s | 472.3 s | 236.1 s | 708.4 s | 0.0% | 0.0% |
| 2,500,000 | committee-hierarchical | 2.24 s | 4.18 s | 5.87 s | 4.47 s | 2.24 s | 6.71 s | 99.2% | 100.0% |
| 2,500,000 | central-collector | 221.6 s | 438.7 s | 650.0 s | 443.2 s | 221.6 s | 664.8 s | 0.0% | 0.0% |
| 2,500,000 | sequential-chain | 1,029,681.0 s | 1,606,767.3 s | 1,907,636.4 s | 2,059,362.0 s | 1,029,681.0 s | 3,089,043.0 s | 0.0% | 0.0% |

### Ethereum Results

The Ethereum node data run produced:

| Validators | Scenario | Node profile | Round | SSF slot | 3SF slot | Local 4s window | 3SF fits 12s |
|---:|---|---|---:|---:|---:|---|---|
| 1,000,000 | ethereum-committee-subnets | all profiles | 8.40 s | 16.80 s | 8.40 s | yes | yes |
| 1,000,000 | global-quorum-collector | ethereum.org minimum, 10 Mbit/s, 2 cores | 294.58 s | 589.16 s | 294.58 s | no | no |
| 1,000,000 | global-quorum-collector | ethereum.org recommended, 25 Mbit/s, 4 cores | 147.29 s | 294.58 s | 147.29 s | no | no |
| 1,000,000 | global-quorum-collector | measured fat node, 10,000 Mbit/s, 32 cores | 18.41 s | 36.82 s | 18.41 s | no | no |
| 2,500,000 | ethereum-committee-subnets | all profiles | 9.60 s | 19.20 s | 9.60 s | no | yes |
| 2,500,000 | global-quorum-collector | ethereum.org minimum, 10 Mbit/s, 2 cores | 736.45 s | 1,472.91 s | 736.45 s | no | no |
| 2,500,000 | global-quorum-collector | ethereum.org recommended, 25 Mbit/s, 4 cores | 368.23 s | 736.45 s | 368.23 s | no | no |
| 2,500,000 | global-quorum-collector | measured fat node, 10,000 Mbit/s, 32 cores | 46.03 s | 92.06 s | 46.03 s | no | no |

## Main Interpretation

### The evidence floor is small

The `evidence-lower-bound` row stays well below 12 seconds even at 2.5 million validators. Its median one-round value is only 475.2 ms at 2.5 million validators.

This supports the lower-bound perspective in the SoK paper: the raw information requirement matters, but compressed accounting evidence alone is not the whole bottleneck.

### Flat GossipSub is not viable at scale

The corrected flat GossipSub models do not fit the 12 second slot budget at large validator counts.

At 1 million validators:

- conservative GossipSub p50 round: 212.2 s
- optimized GossipSub p50 round: 94.1 s

Since SSF needs two rounds and 3SF needs one round per slot, neither protocol fits over flat all-validator GossipSub in this model.

This is consistent with Fujihara's results which emphasizes that GossipSub mesh degree, congestion, duplicate forwarding, and effective parallelism affect aggregation time beyond the information-theoretic lower bound.

### Committee-hierarchical aggregation is the viable path

The `committee-hierarchical` topology consistently fits the 12 second slot budget.

At 2.5 million validators:

- p50 aggregation round: 2.24 s
- p90 aggregation round: 4.18 s
- p99 aggregation round: 5.87 s
- SSF fit rate: 99.2%
- 3SF fit rate: 100.0%

This suggests that the key design requirement is:

> use an aggregation architecture that makes the global vote smaller before it reaches the global layer.

This aligns with IEEE SoK, which argues that fast-finality feasibility depends on solving the signature aggregation bottleneck and reducing the validator-key communication burden.

The Ethereum node experiment sharpens this point. When aggregation follows current style committee/subnet structure, a one-round 3SF-style slot fits the 12 second slot budget in the deterministic model. At 1 million validators the modeled round is 8.40 seconds; at 2.5 million validators it is 9.60 seconds.

However, SSF still fails in that same setup because SSF needs two aggregation rounds in one slot. The modeled SSF slot cost is 16.80 seconds at 1 million validators and 19.20 seconds at 2.5 million validators.

The local 4 second aggregation window also becomes stressed at very large validator counts. At 2.5 million validators, the modeled local committee aggregation step is 4.80 seconds, which misses the 4 second gap between attestation publication and aggregate publication. This means the 3SF slot-level result is promising but not a complete production design.

### Central collection fails

The `central-collector` topology fails for all large validator counts. At 1 million validators, its p50 one-round time is 90.8 seconds.

This is expected: forcing one participant to directly receive and process quorum votes reintroduces the bandwidth and verification bottleneck.

The deterministic Ethereum node-level stress case reaches the same conclusion. Under `global-quorum-collector`, 1 million validators take 147.29 seconds on the ethereum.org recommended node profile and still take 18.41 seconds even on the measured 10,000 Mbit/s, 32-core fat-node profile. At 2.5 million validators, the fat-node profile rises to 46.03 seconds.

So the result is not just "home bandwidth is too low." Direct global collection is structurally the wrong aggregation shape for million-validator fast finality.

## Comparison To The Papers

### Agreement with IEEE SoK

IEEE SoK paper makes four points that the results support:

1. SSF gives same-slot finality but carries high per-slot communication pressure.
2. 3SF reduces per-slot communication by collapsing two voting phases into one.
3. Signature aggregation latency is the dominant practical bottleneck for large validator sets.
4. Validator-key count matters because communication and aggregation scale with validator keys, not merely independent operators.

Our results reproduce that structure:

- SSF is always exactly twice as expensive per slot as 3SF for the same topology.
- 3SF helps per-slot feasibility, but flat GossipSub still fails at scale.
- Hierarchical aggregation is required for feasibility in the sampled range.

### Agreement with Fujihara's Results

Fujihara paper argues that aggregation time should combine:

- an information-theoretic lower bound, and
- a CTMC/topology effect from GossipSub, mesh degree, effective parallelism, congestion, and duplicate forwarding.

The corrected model follows that:

```text
round_ms = max(evidence_floor, propagation_cost, topology_specific_crypto_or_collection_cost)
```

The results also match the meeting note’s qualitative conclusion:

```text
minimum possible aggregation time << practical GossipSub aggregation time
```

The lower bound is small, but network topology and aggregation architecture decide practical feasibility.

# Conclusion

A Monte Carlo sensitivity model over bandwidth, evidence size, propagation latency, duplicate forwarding, CPU variation, quorum size, and aggregation topology shows that fast-finality feasibility is dominated by aggregation architecture. A deterministic Ethereum node-level profile using mainnet slot timing, consensus GossipSub constants, and published node bandwidth profiles reaches the same architectural conclusion: current-style committee/subnet aggregation can keep a one-round 3SF-style aggregation inside the 12 second slot budget, but SSF's two-round slot budget and direct global quorum collection do not fit at million-validator scale. Fast finality therefore requires hierarchical or committee-based aggregation rather than flat all-validator gossip or a single global collector.

## Sources and attribution

This repository combines an inherited signature-aggregation simulator with
new standalone SSF/3SF sensitivity experiments.

We reconfigure and heavily base our work on the open sourced code of Wonderboom, as introduced in the paper by Avarikioti et al. In fact most of this codebase has been a direct fork of that code albeit with our changes.

Sources that are the best to get upto speed for this work:

1. Zeta Avarikioti, Ray Neiheiser, Krzysztof Pietrzak, and Michelle X. Yeo,
   "Wonderboom -- Efficient, and Censorship-Resilient Signature Aggregation for
   Million Scale Consensus," arXiv:2602.06655, 2026. DOI:
   [10.48550/arXiv.2602.06655](https://doi.org/10.48550/arXiv.2602.06655).
   This is the Wonderboom paper behind the million-validator signature
   aggregation simulator context. 
2. Yash Saraswat and Abhimanyu Nag, "SoK: Speedy Secure Finality,"
   arXiv:2512.20715, 2025. DOI:
   [10.48550/arXiv.2512.20715](https://doi.org/10.48550/arXiv.2512.20715).
   The local copy used while preparing the report is
   `/home/abhimanyu/Downloads/IEEEFinal.pdf`. This source provides the SSF and
   3SF protocol framing used by the standalone comparison.
3. Akihiro Fujihara, "Exploring the Universality of Finality Time in
   Proof-Of-Stake Blockchains: Empirical Analysis and Mathematical Formulation
   of Size-Synchrony Antagonism," IEEE ICBC 2025,
   [10.1109/ICBC64466.2025.11114702](https://doi.org/10.1109/ICBC64466.2025.11114702).
4. Codebase reference: this checkout is linked to
   [AbhiMan1601/Signature-Aggregation-Simulation](https://github.com/AbhiMan1601/Signature-Aggregation-Simulation)
   as its Git remote. The inherited `tree` simulator path and BLS aggregation
   constants come from that codebase, we developed the`compare_topologies`
   runner, Docker results entrypoint (using their Dockerfile), and the artifacts in `reports/` are
   the SSF/3SF experiment additions in this repository.

## APPENDIX: Original live simulator

The default binary accepts:

```sh
cargo run --release -- <fanout_m> <cores> <participation_modifier>
```

Example:

```sh
cargo run --release -- 512 4 6
```

The live simulator expects the hostnames `leaf`, `aggregator`, and `leader` to
resolve locally. The Docker entrypoint sets those names up and adds 100 ms
netem delay:

```sh
docker build -t wonderboom-ethereum .
docker run --rm --cap-add NET_ADMIN wonderboom-ethereum
```

## APPENDIX: Docker standalone SSF/3SF results

The same Docker image also includes a separate entrypoint for the standalone
finality sweep:

```sh
docker build -t ethereum-finality-sim .
docker run --rm --entrypoint /usr/local/bin/run-results.sh ethereum-finality-sim
```

CSV output:

```sh
docker run --rm \
  -e FORMAT=csv \
  --entrypoint /usr/local/bin/run-results.sh \
  ethereum-finality-sim
```

Choose validator counts:

```sh
docker run --rm \
  -e VALIDATORS=128000,512000,1000000,2097152 \
  --entrypoint /usr/local/bin/run-results.sh \
  ethereum-finality-sim
```

You can also pass `compare_topologies` flags directly:

```sh
docker run --rm \
  --entrypoint /usr/local/bin/run-results.sh \
  ethereum-finality-sim \
  --validators 128000,512000,1000000,2097152 --trials 2000 --seed 42 --csv
```
