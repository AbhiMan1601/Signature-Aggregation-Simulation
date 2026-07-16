# SSF and 3SF Aggregation Sensitivity Report

Date: July 15, 2026

## Executive Summary

We ran a Dockerized Monte Carlo sensitivity experiment for Ethereum fast-finality aggregation. The experiment compares single-slot finality (SSF) and three-slot finality (3SF) under different network and aggregation topologies.

The key result is:

> 3SF reduces per-slot aggregation pressure relative to SSF, but neither SSF nor 3SF is viable at million-validator scale over flat all-validator GossipSub in this model. Fast finality becomes viable only when the aggregation topology is hierarchical or committee-based.

This matches the theoretical direction in the two reference PDFs:

- [IEEEFinal.pdf](/home/abhimanyu/Downloads/IEEEFinal.pdf): SSF has two voting phases per slot, while 3SF merges the head vote and FFG vote into one voting phase per slot. The paper identifies signature aggregation latency and validator-key count as the dominant bottlenecks.
- [20260709meeting.pdf](/home/abhimanyu/Downloads/20260709meeting.pdf): finality time should be understood using both an information-theoretic lower bound and a topology/CTMC model that accounts for GossipSub mesh degree, effective parallelism, congestion, and duplicate forwarding.

The results should be used as a reproducible sensitivity study, not as final Ethereum mainnet measurements.

## 1. What We Ran

The Docker command used was:

```sh
sudo docker build -t ethereum-finality-sim .
sudo docker run --rm \
  --entrypoint /usr/local/bin/run-results.sh \
  ethereum-finality-sim \
  --trials 2000 --seed 42 --csv
```

The Docker build completed successfully. Docker printed:

```text
WARNING: current commit information was not captured by the build
```

This is harmless. It only means Docker could not attach Git metadata because the current directory is not a Git repository.

The simulation output was CSV with:

```text
trials = 2000
seed   = 42
```

The exact CSV is saved at:

```text
reports/finality-monte-carlo-results.csv
```

## 2. Why This Experiment Exists

The theoretical question is not simply whether SSF or 3SF is “faster.” They optimize different things.

According to [IEEEFinal.pdf](/home/abhimanyu/Downloads/IEEEFinal.pdf), SSF aims to finalize in one slot but requires two voting phases in that slot: a head vote and an FFG vote. The same PDF explains that 3SF merges those voting roles into one vote per slot, reducing per-slot communication while increasing finality depth to three slots.

So the relevant engineering question is:

> Can the required aggregation round or rounds fit inside the slot budget?

The July 9 meeting notes, [20260709meeting.pdf](/home/abhimanyu/Downloads/20260709meeting.pdf), frame this using two complementary models:

1. An information-theoretic lower bound determined by vote information size and bounded bandwidth.
2. A CTMC/topology model that captures GossipSub mesh degree, effective parallelism, topology, aggregation parallelism, congestion, and duplicate forwarding.

The updated simulator follows that structure: every topology is constrained by an evidence-transfer floor, and topology-specific costs are added through propagation, congestion, crypto, and collection terms.

## 3. Protocol Mapping

The simulator first estimates one aggregation round:

```text
round_ms
```

Then it maps this to SSF and 3SF:

```text
SSF_slot        = 2 * round_ms
3SF_slot        = 1 * round_ms
3SF_finality    = 3 * round_ms
```

This follows the protocol structure in [IEEEFinal.pdf](/home/abhimanyu/Downloads/IEEEFinal.pdf):

- SSF has two voting/aggregation phases in the slot.
- 3SF has one voting/aggregation phase per slot.
- 3SF finality occurs across three slots.

The feasibility threshold is:

```text
slot_budget = 12,000 ms
```

So:

```text
SSF fits if 2 * round_ms <= 12,000
3SF fits per-slot if round_ms <= 12,000
```

## 4. Corrected Model

The first version of the topology model was too optimistic because flat GossipSub was modeled as only:

```text
hops * hop_latency
```

That was not good enough because it ignored bandwidth, duplicate forwarding, raw vote size, crypto cost, and quorum variability.

The corrected model uses:

```text
round_ms = max(evidence_floor, propagation_cost, topology_specific_crypto_or_collection_cost)
```

This enforces the key idea from [20260709meeting.pdf](/home/abhimanyu/Downloads/20260709meeting.pdf): the information-theoretic lower bound determines the minimum possible aggregation time, while topology and CTMC-style effects add delay above that floor.

## 5. Randomized Parameter Ranges

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

These are not claimed as exact Ethereum measurements. They are plausible sensitivity ranges designed to stress the theoretical relationships from the PDFs.

## 6. Topologies Tested

| Topology | Interpretation |
|---|---|
| `evidence-lower-bound` | Compressed evidence transfer floor |
| `gossipsub-conservative` | Flat GossipSub with low effective parallelism |
| `gossipsub-optimized` | Flat GossipSub with higher effective parallelism |
| `committee-hierarchical` | Local committee aggregation followed by higher-level aggregation |
| `central-collector` | One participant collects quorum votes directly |
| `sequential-chain` | Sequential forwarding baseline |

This topology set directly reflects [20260709meeting.pdf](/home/abhimanyu/Downloads/20260709meeting.pdf), which highlights GossipSub mesh degree `D`, effective parallel communication `i_c`, topology, aggregation parallelism, and congestion as core variables.

## 7. Results

### 7.1 Full CSV Summary

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

## 8. Main Interpretation

### 8.1 The evidence floor is small

The `evidence-lower-bound` row stays well below 12 seconds even at 2.5 million validators. Its median one-round value is only 475.2 ms at 2.5 million validators.

This supports the lower-bound perspective in [IEEEFinal.pdf](/home/abhimanyu/Downloads/IEEEFinal.pdf): the raw information requirement matters, but compressed accounting evidence alone is not the whole bottleneck.

### 8.2 Flat GossipSub is not viable at scale

The corrected flat GossipSub models do not fit the 12 second slot budget at large validator counts.

At 1 million validators:

- conservative GossipSub p50 round: 212.2 s
- optimized GossipSub p50 round: 94.1 s

Since SSF needs two rounds and 3SF needs one round per slot, neither protocol fits over flat all-validator GossipSub in this model.

This is consistent with [20260709meeting.pdf](/home/abhimanyu/Downloads/20260709meeting.pdf), which emphasizes that GossipSub mesh degree, congestion, duplicate forwarding, and effective parallelism affect aggregation time beyond the information-theoretic lower bound.

### 8.3 Committee-hierarchical aggregation is the viable path

The `committee-hierarchical` topology consistently fits the 12 second slot budget.

At 2.5 million validators:

- p50 aggregation round: 2.24 s
- p90 aggregation round: 4.18 s
- p99 aggregation round: 5.87 s
- SSF fit rate: 99.2%
- 3SF fit rate: 100.0%

This suggests that the key design requirement is not merely “choose 3SF.” The key requirement is:

> use an aggregation architecture that makes the global vote smaller before it reaches the global layer.

This aligns with [IEEEFinal.pdf](/home/abhimanyu/Downloads/IEEEFinal.pdf), which argues that fast-finality feasibility depends on solving the signature aggregation bottleneck and reducing the validator-key communication burden.

### 8.4 Central collection fails

The `central-collector` topology fails for all large validator counts. At 1 million validators, its p50 one-round time is 90.8 seconds.

This is expected: forcing one participant to directly receive and process quorum votes reintroduces the bandwidth and verification bottleneck.

### 8.5 Sequential forwarding is only a negative baseline

The `sequential-chain` topology is many orders of magnitude too slow. It is included only to show that aggregation must be parallelized.

## 9. Comparison To The PDFs

### 9.1 Agreement with IEEEFinal.pdf

[IEEEFinal.pdf](/home/abhimanyu/Downloads/IEEEFinal.pdf) makes four points that the results support:

1. SSF gives same-slot finality but carries high per-slot communication pressure.
2. 3SF reduces per-slot communication by collapsing two voting phases into one.
3. Signature aggregation latency is the dominant practical bottleneck for large validator sets.
4. Validator-key count matters because communication and aggregation scale with validator keys, not merely independent operators.

Our results reproduce that structure:

- SSF is always exactly twice as expensive per slot as 3SF for the same topology.
- 3SF helps per-slot feasibility, but flat GossipSub still fails at scale.
- Hierarchical aggregation is required for feasibility in the sampled range.

### 9.2 Agreement with 20260709meeting.pdf

[20260709meeting.pdf](/home/abhimanyu/Downloads/20260709meeting.pdf) argues that aggregation time should combine:

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

## 10. Is This A Good Feature For The Paper?

Yes, but it should be presented carefully.

This is a good paper feature because it gives a reproducible, Dockerized sensitivity study that connects the theoretical discussion to concrete parameter sweeps.

It should not be presented as:

> proof that 3SF is deployable on Ethereum as-is.

It should be presented as:

> evidence that 3SF reduces per-slot aggregation demand, but scalable fast finality also requires hierarchical or committee-based aggregation.

That is a stronger and more defensible claim.

## 11. Paper-Safe Claim

A paper-safe version of the conclusion is:

> A Monte Carlo sensitivity model over bandwidth, evidence size, propagation latency, duplicate forwarding, CPU variation, quorum size, and aggregation topology shows that fast-finality feasibility is dominated by aggregation architecture. 3SF halves the per-slot aggregation burden relative to SSF, but flat all-validator GossipSub remains insufficient at million-validator scale under the sampled assumptions. Committee-hierarchical aggregation is the only modeled nontrivial topology that consistently keeps both SSF and 3SF within a 12 second slot.

## 12. Limitations

These results are useful, but they are not final empirical measurements.

Limitations:

- The sampled parameter ranges are plausible, not measured from live Ethereum nodes.
- GossipSub behavior is simplified.
- The CTMC model is approximated through randomized topology parameters rather than fully simulated as a stochastic process.
- Adversarial behavior is not modeled.
- Queueing is approximated through a duplicate/congestion factor.
- The committee-hierarchical topology is generic and does not specify a production-ready protocol.
- The cryptographic constants come from the existing codebase’s BLS benchmark constants.

## 13. Recommended Next Steps

Before using this as a central experimental result in the paper:

1. Calibrate bandwidth and latency ranges from Ethereum networking measurements.
2. Estimate realistic GossipSub duplicate factors.
3. Add plots for p50/p90/p99 by validator count.
4. Run multiple seeds and report confidence intervals.
5. Validate `committee-hierarchical` against a more detailed event/network simulator.
6. Separate compressed bitfield/evidence transmission from raw vote propagation in more detail.

## 14. Final Conclusion

The corrected results are viable as a paper feature if framed as a sensitivity study.

They support the theoretical message from the two PDFs:

- SSF optimizes finality depth but maximizes per-slot communication.
- 3SF reduces per-slot communication but still depends on solving aggregation.
- The information-theoretic lower bound is not the main obstacle by itself.
- GossipSub/topology effects dominate practical latency.
- Hierarchical aggregation is the most promising modeled direction.

The strongest conclusion is therefore:

> 3SF is more aggregation-friendly than SSF, but topology is the real deciding factor. At Ethereum-scale validator counts, fast finality requires a hierarchical aggregation design, not flat all-validator gossip or centralized collection.
