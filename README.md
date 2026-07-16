# Ethereum Fast-Finality Aggregation Experiments

This repository contains two useful ways to study signature aggregation:

1. `tree` (the default binary): the original async TCP simulator. It runs one
   measured validator/aggregator path and simulates the surrounding peers.
2. `compare_topologies`: a fast analytical runner for standalone single-slot
   finality (SSF) and three-slot finality (3SF) sweeps over validator counts and
   network topology assumptions.

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

## Current experiment results

The latest Dockerized Monte Carlo sweep is saved in:

- `reports/finality-results-report.md`
- `reports/finality-monte-carlo-results.csv`

The experiment ran 2,000 trials with seed `42` and compared SSF and 3SF under
different aggregation topologies. The main result is that 3SF reduces per-slot
aggregation pressure relative to SSF, but flat all-validator GossipSub is still
too slow at million-validator scale in this model. Fast finality only fits the
12 second slot budget when aggregation is hierarchical or committee-based.

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

Paper-safe conclusion:

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
