# Ethereum Fast-Finality Aggregation Experiments

This repository contains work on how to simulate and study signature aggregation:

1. `tree` (the default binary): the original async TCP simulator. It runs one
   measured validator/aggregator path and simulates the surrounding peers.
2. `compare_topologies`: a fast analytical runner for standalone single-slot
   finality (SSF) and three-slot finality (3SF) sweeps over validator counts and
   network topology assumptions.
3. `ethereum_node_latency`: a deterministic runner that applies Ethereum
   mainnet slot/gossip timing and node-level bandwidth profiles to signature
   aggregation scenarios.

## Sources and attribution

This repository combines an inherited signature-aggregation simulator with
new standalone SSF/3SF sensitivity experiments.

Primary sources:

1. Yash Saraswat and Abhimanyu Nag, "SoK: Speedy Secure Finality,"
   arXiv:2512.20715, 2025. DOI:
   [10.48550/arXiv.2512.20715](https://doi.org/10.48550/arXiv.2512.20715).
   The local copy used while preparing the report is
   `/home/abhimanyu/Downloads/IEEEFinal.pdf`. This source provides the SSF and
   3SF protocol framing used by the standalone comparison.
2. Zeta Avarikioti, Ray Neiheiser, Krzysztof Pietrzak, and Michelle X. Yeo,
   "Wonderboom -- Efficient, and Censorship-Resilient Signature Aggregation for
   Million Scale Consensus," arXiv:2602.06655, 2026. DOI:
   [10.48550/arXiv.2602.06655](https://doi.org/10.48550/arXiv.2602.06655).
   This is the Wonderboom paper behind the million-validator signature
   aggregation simulator context.
3. Akihiro Fujihara, "Meeting notes on July 9, 2026," local file
   `/home/abhimanyu/Downloads/20260709meeting.pdf`. These notes motivate the
   lower-bound plus topology/CTMC-style interpretation used in the report, and
   cite: Akihiro Fujihara, "Exploring the Universality of Finality Time in
   Proof-Of-Stake Blockchains: Empirical Analysis and Mathematical Formulation
   of Size-Synchrony Antagonism," IEEE ICBC 2025,
   [10.1109/ICBC64466.2025.11114702](https://doi.org/10.1109/ICBC64466.2025.11114702).
4. Codebase reference: this checkout is linked to
   [AbhiMan1601/Signature-Aggregation-Simulation](https://github.com/AbhiMan1601/Signature-Aggregation-Simulation)
   as its Git remote. The inherited `tree` simulator path and BLS aggregation
   constants come from that codebase; the standalone `compare_topologies`
   runner, Docker results entrypoint, and report artifacts in `reports/` are
   the SSF/3SF experiment additions in this repository.
5. Ethereum protocol constants: the
   [Ethereum consensus specs mainnet config](https://github.com/ethereum/consensus-specs/blob/dev/configs/mainnet.yaml)
   defines a 12 second mainnet slot, and the
   [consensus P2P interface](https://github.com/ethereum/consensus-specs/blob/dev/specs/phase0/p2p-interface.md)
   defines the beacon-chain GossipSub parameters used here: mesh target `D=8`,
   heartbeat interval `0.7s`, maximum gossip clock disparity `500ms`, 64
   attestation subnets, and 2 long-lived subnets per node. The
   [honest validator spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/phase0/validator.md)
   broadcasts attestations at `SECONDS_PER_SLOT / INTERVALS_PER_SLOT` and
   aggregate attestations at `2 * SECONDS_PER_SLOT / INTERVALS_PER_SLOT`,
   which is 4s and 8s respectively under the mainnet 12s slot and
   `INTERVALS_PER_SLOT=3`.
6. Ethereum node bandwidth profiles: ethereum.org's
   [run a node guide](https://ethereum.org/developers/docs/nodes-and-clients/run-a-node/)
   lists 10+ MBit/s as a minimum and 25+ MBit/s as recommended bandwidth.
   Cortes-Goicoechea, Mohandas-Daryanani, Munoz-Tapia, and Bautista-Gomez,
   ["Can we run our Ethereum nodes at home?"](https://arxiv.org/abs/2311.05252),
   arXiv:2311.05252, provide measured control-machine profiles with
   100 Mbit/s, 250 Mbit/s, and 10,000 Mbit/s network links.

The results below should therefore be cited as a reproducible sensitivity study
built on the above sources, not as measurements directly reported by the
original Wonderboom paper.

## Fast topology comparison

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

## Ethereum node-level latency and bandwidth simulator

Run the deterministic Ethereum node profile simulator:

```sh
cargo run --release --bin ethereum_node_latency
```

Export CSV:

```sh
cargo run --release --bin ethereum_node_latency -- --csv
```

Choose validator counts:

```sh
cargo run --release --bin ethereum_node_latency -- --validators 128000,512000,1000000,2500000 --csv
```

The generated CSV is saved in:

- `reports/ethereum-node-latency-results.csv`

Methodology:

- Slot timing uses Ethereum mainnet's 12 second slot.
- Attestation broadcast is modeled at 4 seconds into the slot and aggregate
  broadcast at 8 seconds into the slot, matching the honest-validator timing
  rule under `INTERVALS_PER_SLOT=3`.
- Gossip propagation uses the consensus P2P constants `D=8`,
  `heartbeat_interval=0.7s`, and `MAXIMUM_GOSSIP_CLOCK_DISPARITY=500ms`.
- Bandwidth profiles are fixed to 10, 25, 100, 250, and 10,000 Mbit/s from the
  ethereum.org node guide and the Cortes-Goicoechea et al. node-resource study.
- Crypto cost uses the BLS benchmark constants already present in this
  codebase: verify, aggregate signature, and aggregate public key cost per
  validator.
- Two scenarios are tested. `ethereum-committee-subnets` follows current
  Ethereum-style committee/subnet aggregation with 32 slots per epoch, up to 64
  committees per slot, target committee size 128, max committee size 2,048, and
  16 target aggregators per committee. `global-quorum-collector` is a stress
  case where one node must collect and aggregate quorum votes from the whole
  validator set.

Representative deterministic results:

| Validators | Scenario | Node profile | Round | SSF slot | 3SF slot | 4s local window | 3SF fits 12s |
|---:|---|---|---:|---:|---:|---|---|
| 1,000,000 | ethereum-committee-subnets | all profiles | 8.40 s | 16.80 s | 8.40 s | yes | yes |
| 1,000,000 | global-quorum-collector | ethereum.org minimum, 10 Mbit/s, 2 cores | 294.58 s | 589.16 s | 294.58 s | no | no |
| 1,000,000 | global-quorum-collector | ethereum.org recommended, 25 Mbit/s, 4 cores | 147.29 s | 294.58 s | 147.29 s | no | no |
| 1,000,000 | global-quorum-collector | measured fat node, 10,000 Mbit/s, 32 cores | 18.41 s | 36.82 s | 18.41 s | no | no |
| 2,500,000 | ethereum-committee-subnets | all profiles | 9.60 s | 19.20 s | 9.60 s | no | yes |
| 2,500,000 | global-quorum-collector | ethereum.org minimum, 10 Mbit/s, 2 cores | 736.45 s | 1,472.91 s | 736.45 s | no | no |
| 2,500,000 | global-quorum-collector | ethereum.org recommended, 25 Mbit/s, 4 cores | 368.23 s | 736.45 s | 368.23 s | no | no |
| 2,500,000 | global-quorum-collector | measured fat node, 10,000 Mbit/s, 32 cores | 46.03 s | 92.06 s | 46.03 s | no | no |

Interpretation:

- The current Ethereum-style committee/subnet shape is the only modeled
  Ethereum-node setup that stays close to the slot budget. At 1,000,000
  validators, the modeled round is 8.40 seconds, so a one-round 3SF-style slot
  fits within 12 seconds. At 2,500,000 validators, the round is 9.60 seconds,
  so 3SF still fits the slot-level budget.
- SSF does not fit this deterministic Ethereum-node model because SSF needs two
  aggregation rounds inside one 12 second slot. The committee/subnet scenario
  becomes 16.80 seconds at 1,000,000 validators and 19.20 seconds at 2,500,000
  validators.
- The local 4 second aggregation window is a tighter constraint than the full
  12 second slot. At 2,500,000 validators, the committee/subnet local
  aggregation step is modeled at 4.80 seconds, so it misses the 4 second gap
  between attestation publication and aggregate publication. This means the
  3SF slot-level result is promising, but it is not a complete production-ready
  protocol claim.
- Bandwidth upgrades alone do not save the global collector design. At
  1,000,000 validators, direct global quorum collection takes 147.29 seconds on
  the ethereum.org recommended node profile and 18.41 seconds even on the
  10,000 Mbit/s, 32-core fat-node profile. At 2,500,000 validators, the fat-node
  profile still takes 46.03 seconds.
- The practical conclusion is that Ethereum-scale fast finality needs
  hierarchical or committee-based aggregation. A single node collecting quorum
  votes from the whole validator set is structurally too slow, even with strong
  bandwidth and CPU assumptions.

## Current experiment results

The latest Dockerized Monte Carlo sweep is saved in:

- `reports/finality-results-report.md`
- `reports/finality-monte-carlo-results.csv`

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

> A Monte Carlo sensitivity model over bandwidth, evidence size, propagation
> latency, duplicate forwarding, CPU variation, quorum size, and aggregation
> topology shows that fast-finality feasibility is dominated by aggregation
> architecture. 3SF halves the per-slot aggregation burden relative to SSF, but
> flat all-validator GossipSub remains insufficient at million-validator scale
> under the sampled assumptions. Committee-hierarchical aggregation is the only
> modeled nontrivial topology that consistently keeps both SSF and 3SF within a
> 12 second slot.

## Original live simulator

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

Fresh live runs may spend substantial time generating and caching BLS key/signature
material in `data/signatures.bin`.

Sample live simulator timings from `ethereumresult.txt`:

| Fanout `m` | Cores | Runs | Average final time (ms) |
|---:|---:|---|---:|
| 512 | 4 | 17,021; 16,525; 16,664; 17,006; 16,539 | 16,751.0 |
| 650 | 4 | 17,686; 17,546; 17,542; 17,610; 17,563 | 17,589.4 |
| 820 | 4 | 18,944; 18,327; 18,324; 18,298; 18,277 | 18,434.0 |
| 1018 | 4 | 19,282; 19,444; 19,540; 19,612; 20,014 | 19,578.4 |
| 1250 | 4 | 22,070; 21,309; 21,300; 21,289; 21,337 | 21,461.0 |

## Docker standalone SSF/3SF results

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

Run the Ethereum node-level profile simulator inside the image:

```sh
docker run --rm \
  --entrypoint /usr/local/bin/ethereum_node_latency \
  ethereum-finality-sim \
  --validators 128000,512000,1000000,2500000 --csv
```
