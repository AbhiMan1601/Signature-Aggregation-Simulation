# Experimental companion and manuscript audit

The manuscript's N log N behavior is conditional on saturation of **network-wide effective capacity**. It is not established as a universal lower bound for fast finality. The exact CTMC can be implemented consistently while failing to predict an independently specified network. This companion keeps mathematical verification, independent validation, and conditional architecture timing separate.

## Scope and provenance

The pasted attachment supplied the experimental specification; the manuscript supplied equations and claims to test, rather than instructions governing repository actions. The working tree was initially clean. No applicable AGENTS.md was found in the repository or its parent directories. Baseline commit: `41ce5854661b3383e5e59cdd2fd4173137197533`. Manuscript: anonymous, tentative title “Is Single-Slot Finality Really Fast? On Signature Aggregation Latency in Ethereum Fast-Finality Protocols,” SHA-256 `59148694ef16ba0ec2e5c613b96789268fdd5142554e80ceffa5fdd80897f69c`. All six manuscript pages were extracted; rendered page 4 was inspected to confirm equations (8), (12), (16), (18), and (19).

The repository-backed implementation is one standalone Rust binary, `src/bin/paper_ctmc.rs`, with one Python execution/analysis/plotting script. The script's `PLAN` is the versioned configuration; expanded configurations and their hashes are also saved with observations. The only dependency addition is serde_json with its locked dependencies. The audit and LaTeX draft share this document, and five figures share one panel composition exported in three formats to reduce file changes. No legacy CSVs, figures, or simulator implementations were overwritten. This is a local experiment; no push, deployment, paid computation, or host networking changes were performed.

`reports/paper-results.json.gz` contains raw trajectories, network trials, exact and empirical moments, confidence intervals, seeds, configuration/source hashes, BLS calibration samples, prediction trials, run times, compile settings, CPU/platform details, toolchain versions, and analyzed summaries. It is a portable JSON dataset compressed losslessly. The source hashes identify the uncommitted experimental implementation separately from the baseline commit. Preliminary runs were retained at `/tmp/icbc-pilot-results.json.gz` and `/tmp/icbc-pre-schedule-results.json.gz`; only the final repository dataset supports the numerical results here.

## Methods

### A. Mathematical verification

`Config::population`, `quorum`, `rates`, `closed`, `moments`, `sample`, and `ctmc` implement equations (4), (8), (12), and (16). Seconds are used throughout. `quorum(N)=N-N/3` safely computes ceil(2N/3) without multiplication overflow. This counts **equal-weight validator identities**, not stake. Either integer A or a fractional availability may be supplied; A=floor(a N), with A=0 rejected. Broadcast always uses N, as specified by the manuscript, even when A<N. There is no silent substitution of A into the broadcast model.

N=1 and Q=1 give empty sums and zero completion times. Q>A produces `attainable=false` and absent aggregation results, while preserving broadcast observations. Q=A uses the exact harmonics including H0=0. Invalid/nonfinite rates and moment overflow are errors. A resource guard limits exact sampling to 200 million state visits per request; moments-only requests use `trials=0`. No normal or grouped trajectory approximation is used.

Closed-form harmonic means and inverse-rate moments are evaluated separately with compensated summation. Exponential holding times are sampled by inverse transform. Broadcast and aggregation use separate ChaCha8 streams. Reusing a seed across parameter configurations deliberately couples underlying uniforms for sensitivity comparisons; thus statistical validation counts across configurations are correlated. This pairing is not claimed as independent replication. Three seeds supply 6,000 exact trajectories per sampled scaling configuration. Exact moments cover N up to one million under ic=8, sqrt(N), and N. Sampling covers N≤4,096. Additional experiments vary lambda, degree, fixed-global-quorum availability, Q=A, Q>A, and Q=1.

Means have Monte Carlo standard errors and approximate normal 95% intervals, justified by thousands of independent trajectories per request. Sample variances and p50/p95/p99 are saved. Deadline probabilities have Wilson binomial intervals, including nondegenerate bounds when no failures are observed. A stochastic discrepancy is reported, never used as a narrow pass/fail test. Deterministic tests compare inverse-rate means and closed forms across small/boundary/representative populations and include a hand-calculated two-node exponential case.

For fixed positive quorum fraction, positive constant availability and lambda,D, and 1≤ic≤N, the formula gives broadcast Theta((1+N/ic) log N). Hence fixed ic yields Theta(N log N), ic=Theta(sqrt N) yields Theta(sqrt N log N), and ic=Theta(N) yields Theta(log N). The plotted normalized curves illustrate this derivation; finite-range observations do not prove an asymptotic theorem. At Q=A the exact finite expression remains well-defined. Equation (17)'s approximation with -log(1-rho) cannot be used there. The integer availability threshold is ceil(2N/3)/N, rather than invariably exactly 2/3.

### B. Independent network validation

`Engine::new`, `transfer`, `cpu`, `run_many`, `disjoint_union`, and `network` determine network observations **without using CTMC rates**. Nodes M and keys N are distinct; key k is assigned to node k mod M. The online keys are the prefix 0..A. This creates correlated benign node/key unavailability and keeps node 0 online. Graphs are constructed over online nodes; this represents a functioning online overlay, not random failures applied to a pre-existing disconnected graph. Unreachable-quorum observations remain in the dataset.

The mesh ensemble is a ring backbone plus uniformly sampled undirected random chords to approximately mean degree D. Connectivity is guaranteed by construction. It is not a uniform connected regular-graph ensemble. Chord generation has a bounded attempt count. Hierarchies use a breadth-first fanout-4 tree; central collection uses a star. Their graphs are deterministic. Mesh trials use eight topology seeds and twelve independent bounded-jitter trials per draw. The star/tree use eight independent delay-seed batches on one graph; between-batch variation is not topology variation. Topology and delay seeds are separate fields; matching seeds pair comparable workloads across capacity regimes.

A min-priority event queue orders by timestamp and monotonic event ID. Every node has a FIFO shared upload resource across outgoing links, a shared download reservation resource, and a queue for each CPU worker (one worker in saved baseline runs). Scheduling reserves resources when work is enqueued. Download reservations follow transmission enqueue order, not arrival-time order; a future arrival can cause conservative head-of-line blocking. CPU jobs likewise reserve worker service at enqueue time, with known release times. This is a declared reservation discipline, not a work-conserving NIC/OS scheduler, and limits external validity. A packet first serializes at upload, then propagates for 10ms plus independent Uniform(0,5ms) jitter, then consumes download service. Receive processing and retransmission follow download completion. This store-and-forward convention charges **both** endpoint serialization; it is not a streaming last-bit WAN model. The alternative regime additionally serializes upload through one global 12.5 MB/s resource. Per-node baseline bandwidth is also 12.5 MB/s. Thus aggregate resources can grow with M in the per-node regime but are deliberately restricted in the global regime.

Broadcast eagerly forwards a new message to neighbors except its immediate sender and suppresses retransmission of duplicate receipts. In-flight duplicates still consume capacity and bytes. Completion is all-online-node processed receipt, distinct from quorum collection. This is simplified eager forwarding, **not GossipSub**: no heartbeat, peer scoring, graft/prune, retransmission repairs, or churn is modeled. Counts/times of new useful receipts are retained for conditional diagnostics; they do not make graph geometry disappear.

Central and mesh raw collection route individually signed votes along the breadth-first parent tree to node 0, using 96 signature bytes and 16 identity/header bytes. Raw forwarding does not verify at intermediates; the collector verifies every incoming vote, reconstructs its public key contribution, and merges it. Distinct identities, not packet count, determine quorum. The collector already holds its own generated votes. Benign in-flight work continues after quorum and can contend with proof traffic.

Hierarchical collection sends one disjoint-subtree aggregate per child. Every internal node signs its own keys, verifies incoming child aggregate signatures, reconstructs the child public-key sum, merges, and forwards only after its own keys and **all online child batches** have arrived. This explicitly favors complete-subtree batches rather than an optimized incremental-quorum protocol; it has a straggler penalty and is not advertised as an optimal architecture. Root termination still occurs when received disjoint batches first cover the global quorum. A root certificate may contain more than Q signatures; verification charges the actual represented membership. Every level and the root have CPU/network queues.

All hierarchy aggregates and final certificates use a declared format: 96-byte BLS signature, 16-byte header, and a bitmap of ceil(N/8) bytes indexed by the eligible key set. The bitmap width is fixed by N, not by the number of participants. Sending it across a root link has a real transfer cost for **this format**; it is not an information-theoretic lower bound. The conservative full-width bitmap is used even for small subtrees. No compressed metadata, threshold-signature DKG, arbitrary overlapping-aggregate addition, or cached subtree public-key sums are assumed. Disjoint merging is asserted. Final root verification and verification at every proof recipient are charged. Traffic, duplicate bytes, summed resource queue times, CPU work/utilization, and completion times are exported. Summed queue times are total waiting work, not critical-path latency.

CPU costs are calibrated by a local release-mode BLS loop, rather than inherited constants or machine-profile extrapolation. The loop uses 32 generated registered keys, same-message aggregate verification with group checks, warm key/signature caches, one thread, and 200 samples for signing, verification, public-key addition, and signature merging. Key-generation/setup is outside the calibration timings and that is explicitly their component-benchmark boundary. Timing is the mean of measured calls, not a consensus deadline. Addition costs divide a 32-item fold by 31 and therefore include small fixed overhead. Synthetic network CPU jobs use these values; actual cryptography is not performed for every network event. POP-style same-message aggregation assumes proof-of-possession registration/trusted nonrogue keys; registration/PoP verification is an external prerequisite, not a proven property of arbitrary keys. Separate BLS tests check final aggregate validity and reject a changed message.

Calibration fixes ic=8 in advance, estimates only the identifiable product lambda D from mean broadcast at N=M=32, and evaluates other sizes without refitting. It is done separately per architecture/capacity regime. The same fitted product predicts aggregation; no collector-only refit rescues a mismatch. Exact CTMC sampling at held-out sizes supplies predicted quantiles and deadline probabilities, while raw network trials supply observed ones. Means, variances, quantiles and probability estimates are separate fields. A log-depth broadcast baseline and linear collector baseline share the same smallest-size calibration. All remain conditional fits, not measurements of Ethereum deployments.

The analysis distinguishes fixed-topology stochastic-delay variance from variation of cluster means. Mean/quantile intervals resample independent topology or delay clusters. Eight clusters and 96 network trials per configuration are modest; p99 is explicitly exploratory. Per-fixed-topology deadline intervals use binomial Wilson intervals. A conservative unconditional interval uses the first predeclared trial of each independent draw/batch. The probability figure encloses both that interval and the cluster bootstrap, avoiding a zero-failure claim from a degenerate bootstrap interval.

### C. Protocol schedules and shared-resource load

Primary sources were read, rather than treating the manuscript's round coefficients as a protocol definition:

- D'Amato and Zanolini, [arXiv:2302.12745v1](https://arxiv.org/pdf/2302.12745v1), Algorithm 3 and Section 7: proposal, head voting, FFG voting, and the acknowledgment extension for observer single-slot finality.
- D'Amato et al., [arXiv:2411.00558v1](https://arxiv.org/html/2411.00558v1), Algorithm 7, Sections 6.2 and 7.1: one combined head/FFG vote phase per slot, then fast confirmation and merge; practical aggregated delivery expands a vote phase to two communication steps.

The synthetic SSF timing path is proposal processed by all online nodes → head-vote generation/collection → verified certificate dissemination → FFG vote generation/collection → verified certificate dissemination → acknowledgment generation/collection → final verified proof dissemination. `Engine::run` executes this causal path on the same queues. There is no unsupported max(broadcast,aggregation,CPU) overlap. It is an engineered sequential aggregation-assisted acknowledgment path; it is not an implementation of fork choice, checkpoint rules, or slashing. Its required slot time is the completion time of this strict same-slot path. Fixed round waits and execution/state-validation cost (configured zero) are absent; adding them can increase the budget.

For the selected 3SF practical service schedule, a common Delta must accommodate proposal service P≤Delta and vote-generation/collection/certificate-delivery service V≤2Delta. Proposal begins at 0; combined head/FFG voting begins at Delta; fast confirmation occurs at 3Delta; merge at 4Delta; next proposal at 5Delta. Thus the service-envelope slot is S=5 max(P,V/2). This maximum has an explicit schedule interpretation, not an assertion that causally sequential work overlaps. A block arrives at the beginning of slot s in a steady honest pipeline. The assumed confirmation/justification progression finalizes it at fast confirmation in s+2, giving delay 2S+3Delta. This convention is derived from the selected schedule, not three times a standalone aggregation round. Ideal throughput is one new proposal per S, subject to shared-resource load.

Four adjacent slot workloads are also enqueued together in `run_many` at both S and S/2, sharing CPU, upload/download, and optional global queues. Voting waits until its configured Delta offset and until proposal processing completes. Success requires every slot's modeled verified proof by its fast-confirmation phase at start+3S/5. These experiments test load contention, not checkpoint correctness or consensus liveness. They use compact synthetic proposal/vote/proof messages instead of the complete on-chain history/views; the strict SSF path and 3SF service envelope are conditional engineering analyses. The SSF path is variable-length sequential service whereas the 3SF envelope budgets fixed phase windows, so a ratio between them is not a theorem comparing optimized protocols. Neither timing outcome establishes safety. Missing a deadline is a performance/liveness event under a stated workload, not evidence of conflicting finalized blocks.

## Claim-to-code audit of the baseline

Line references are to the unchanged baseline files. Suspicions were checked against code; one shell-loop suspicion was disproved.

| Issue | Exact implementation | Finding and treatment |
|---|---|---|
| CTMC implementation | `src/bin/compare_topologies.rs::gossipsub_metrics` (247), `committee_hierarchical_metrics` (283), `aggregate_crypto_ms` (390); `src/bin/ethereum_node_latency.rs::evaluate_*` | Logarithmic hop formulas and linear per-item crypto; no beta/alpha CTMC trajectory or inverse-rate moments. Random parameter draws are sensitivity samples, not packet simulations. New CTMC is separate. |
| Hierarchy accounting | `compare_topologies::committee_hierarchical_metrics` (283) | Includes local committee receive/crypto and a hop-depth term, omits internal/root signature verification/merging, key reconstruction, detailed signer handling and final proof delivery. Superseded by queued subtree experiment. |
| Evidence “lower bound” | `compare_topologies::evidence_floor_ms` (239), `sample_params` (198) | Sampled bytes per quorum participant are an assumed transfer cost; no certificate encoding theorem or obligatory bottleneck is proven. New experiment uses explicit signature/header/bitmap bytes. |
| Overlap and protocol multipliers | `compare_topologies::print_csv` (428), `fit_pct` (415), topology `round_ms`; `ethereum_node_latency::evaluate_*` | SSF=2 round, 3SF slot=round/finality=3 round are imposed. Most legacy round estimates take component maxima without a valid schedule. README labels them legacy. New schedules are sourced and explicit. |
| Committee/global quorum | `ethereum_node_latency::evaluate_committee_profile` (149) | Processes N/32 keys per slot with committee caps. This does not supply a global ceil(2N/3) fast-finality quorum. No new experiment substitutes committees for global quorum. |
| Heartbeat/clock delay | `ethereum_node_latency::evaluate_committee_profile` (149), `evaluate_global_quorum_profile` (206) | Sets per-hop gossip delay to heartbeat+clock disparity. Those configuration allowances are not evidence of forwarding service latency. New jitter/propagation is explicit synthetic input. |
| Hardware | `ethereum_node_latency::node_profiles` (101), `aggregate_crypto_ms` (239) | All profiles reuse three crypto constants and divide by cores. Labels such as measured-fat-node do not measure crypto on that machine. New calibration records actual local CPU/toolchain and one worker. |
| Precomputation | `simulation/simulator.rs::Simulator::init` (42), `simulation/main.rs::main` (44) | Generates 2,048 group aggregates and a global aggregate before `time_now`; remote participants largely transmit precomputed signatures. Excluded from protocol evidence. |
| Duplicate leader completion | `simulation/leader_aggregator.rs::LeaderAggregator::run` (60), `connect_to_child_nodes` (123) | Counts runs toward 2048×16, while each aggregate is reverified and emitted 16 times. This is synthetic workload replication, not independent representatives; unique signer quorum is not the termination invariant. Excluded. |
| Final verification | `LeaderAggregator::run` (60), around 101–108 | Final aggregate public key and verification are commented out. Individual incoming aggregate verification does not replace final-certificate validation. Excluded. |
| Socket writes/framing | `simulator::setup_leaf_nodes` (115), `connect_to_leader` (162); `leaf_aggregator::connect_to_leaf_nodes_send` (220), `setup_leader_connection` (294) | Uses `write` and ignores short successful writes. Reads expect fixed frames with machine-width usize IDs; bitmap padding can be interpreted as extra keys, group IDs/key indexing lack validation. No TCP invariants are claimed as repaired. |
| Partition/shutdown | `leaf_aggregator.rs::LeafAggregator::connect` (57), `run` (89), `verify` (167) | ceil(m/workers), overshoot subtractions can underflow or leave empty worker batches. Public-key partitions can also be empty. `run` awaits a second result even if one worker already completed m; `parent_channels.closed()` waits on retained subscriptions. Single-worker shutdown can deadlock. Excluded rather than cosmetically patching an invalid benchmark. |
| Configuration sweep | `run.sh` (22) | `exec` is on the left side of a **pipeline**. In the available `/bin/sh` it runs in a pipeline subprocess and does not replace the loop's parent shell. A harmless three-iteration equivalent printed all three configurations. Thus “exec prevents the sweep” is not supported here. Privileged network setup/error handling and grep hiding failures remain concerns; run.sh was not executed. |
| Hierarchy asymptotics | README and `reports/finality-results-report.md`; `committee_hierarchical_metrics` | With positive fixed bytes-per-validator and fixed bandwidth, the globally imposed evidence floor eventually grows linearly. Claimed logarithmic total round scaling conflicts with the actual max expression. New results make bitmap transfer assumptions explicit. |

The inherited analytical and TCP components are retained as **qualified legacy code**, not repaired or certified. The new component benchmark and independent simulator replace their role in this evidence. The TCP workload was not executed: it requires privileged hostname/network shaping and has correctness/shutdown defects. No partial-write, framing, worker-partition or TCP-end-to-end regression test is falsely reported as passing. Reusing TCP timing for a paper requires resolving these defects and measuring the corrected timing boundary. The document and README supersede the old report's protocol-feasibility claims without rewriting historical data.

## Claim-to-evidence audit and manuscript changes

| Manuscript claim | Evidence category/status | Revision |
|---|---|---|
| Equations (8) and (16) compute CTMC means | Mathematical, verified numerically | Retain, specify domains, initial own signature, integer A/Q and rate units. |
| N log N bottleneck | Mathematical, conditional | State fixed network-wide capacity saturation; general formula is Theta((1+N/ic)log N). Growing aggregate resources give other regimes. |
| Aggregation is reverse broadcast | Modeling assumption, not independently established | Explain why a collector acquiring j signers would increase useful arrival capacity with j; independent queued collection does not inherit that mechanism. |
| Count-only Markov state models a static mesh | Unsupported generally | Graph geometry, informed-set boundary, queues and CPU occupancy are hidden state; counts need empirical sufficiency tests. Conditional traces are diagnostics, not proof of lumpability. |
| Equation (18) means E[TA]=E[TB] | Incorrect as general equality | Replace equality by “same asymptotic order under specified scaling.” Full A=N,Q=N and shared ic is a special exact equality; quorum collection normally differs. |
| Approximation at rho=1 | Boundary needs revision | Use equation (16) at Q=A; -log(1-rho) approximation diverges although finite-A mean is finite. rho=0 conflicts with at least one own signature and a positive quorum. |
| Quorum near availability threshold | Requires explicit handling | Preserve Q=ceil(2N/3); Q>A is unattainable, not an omitted draw or finite delay. Availability and online-quorum fraction are separate. |
| Equation (24), bound on E[1/L] from E[L] | Invalid inference | Jensen gives E[1/L]≥1/E[L] for positive L. An upper bound requires separate inverse-moment/distribution arguments; no universal throughput bound follows from the displayed mean inequality. |
| Short deadlines sacrifice security | Unsupported by timing analysis | Report deadline-success/liveness performance; establish safety separately from protocol assumptions and proof. |
| Validator count measures decentralization | Unsupported equivalence | Separate identities, equal-weight/stake quorum, network nodes, physical resources and independent operators. Multi-key experiments illustrate capacity changes without adding nodes. |
| 3SF constant-factor superiority | Conditional architecture/schedule analysis | Name versions, phase dependencies, acknowledgment convention, pipeline load and arrival/observation points. Remove fixed universal factor of two. |
| PACELCS/BST follows as theorem | Interpretation/hypothesis | Keep as hypothesis consistent with selected resource constraints; neither protocol lower bound nor decentralization/security theorem is proven. |
| “Fundamental lower bound” on admissible protocols | Unsupported | A chosen CTMC's expected completion time is not a theorem quantifying over protocols/networks. Specify capacity model, admissible algorithms and completion criterion before stating a lower bound. |

A simpler baseline beating a fixed-ic fit on a network is useful adverse evidence. It does not establish that a different ic(N) law is universally valid. Lambda and D cannot be separately identified from the CTMC broadcast fit without independent link/degree information. Quantile/probability discrepancies and variance mismatches matter even if mean curves appear similar. No parameters were changed per size to manufacture manuscript agreement.

## Reproduction and checks

From the repository root:

```sh
cargo build --offline --release --bin paper_ctmc
cargo test --offline --bins
cargo clippy --offline --bin paper_ctmc -- -D warnings
rustfmt --check --edition 2021 src/bin/paper_ctmc.rs
python3 -m py_compile scripts/paper_experiments.py
python3 -m venv /tmp/icbc-venv
/tmp/icbc-venv/bin/pip install matplotlib==3.11.2 numpy==2.5.3
/tmp/icbc-venv/bin/python scripts/paper_experiments.py run --output /tmp/paper-reproduction.json.gz
/tmp/icbc-venv/bin/python scripts/paper_experiments.py plot
```

The run action refuses to replace an existing dataset. Supply `--manuscript` when the PDF is elsewhere. Plot reads the named dataset (`--output` overrides it). It regenerates deterministic scientific figures from saved observations; no image generation or hand-entered data is used.

The Rust binary accepts one JSON configuration per input line and returns one JSON result per line, including canonical defaults. Examples:

```sh
printf '%s\n' '{"n":128,"a":96,"q":86,"lambda":1,"degree":8,"ic":8,"trials":2000,"seed":17}' | target/release/paper_ctmc
printf '%s\n' '{"n":1000000,"ic":1000000,"trials":0}' | target/release/paper_ctmc
printf '%s\n' '{"n":1024,"a":614,"trials":2000}' | target/release/paper_ctmc
/tmp/icbc-venv/bin/python scripts/paper_experiments.py single --config '{"mode":"network","n":128,"m":32,"topology":"hierarchy","trials":12}'
```

Standalone network defaults are illustrative synthetic CPU constants; the saved suite instead replaces them with the recorded local BLS calibration. Changing a constant is sensitivity analysis, not an empirical deployment distribution.

Actual verification outcomes: the new deterministic tests passed, the release binary built, all-bin tests passed (two legacy tests plus the new tests), new-binary Clippy with warnings denied passed, targeted rustfmt checking passed, and Python syntax checking passed. The inherited main binary emits pre-existing warnings; no whole-repository clean-lint claim is made. No repository-wide formatting edits were made. Test coverage includes closed forms, two-node moments, invalid inputs, unattainable quorum, zero-time own quorum, one-link serialization, upload/CPU queue scheduling, small broadcast, hierarchy coverage, duplicate rejection, deterministic replay, overlapping shared-resource workloads, and actual final BLS certificate validity/invalid-message rejection.

Large-N trajectory sampling is deliberately bounded. The full moments grid is completed, while million-node Monte Carlo tails and live Ethereum measurements are not. Requesting 2,000 million-node exact trajectories would need roughly two billion state visits, exceeds the runner's sampling guard, and is unnecessary for illustrating scaling. To expand safely, run twenty independently seeded 100-trial configurations; benchmark the first chunk, multiply its elapsed time by twenty and stop at a chosen budget. Streaming only summary results would reduce storage, but the current runner exports every trajectory. Million-node independent network runs are unsupported by the network resource guard and should not be presented as completed.

## Figure captions

`reports/figures/paper-experiments.pdf`, `.svg`, and `.png` contain the same five-panel composition generated from `reports/paper-results.json.gz` by the plot command.

A: Exact broadcast/aggregation means against exact CTMC Monte Carlo for ic=8, lambda=1/s, D=8, A=N, Q=ceil(2N/3). Each N has 6,000 trajectories from three seeds. Error bars are approximate 95% mean intervals using the exact known variance, not parameter-range percentiles.

B: Exact broadcast means normalized by N log N, sqrt(N) log N and log N for ic=8, sqrt(N), N respectively. Lambda=1/s, D=8; N ranges from 16 to one million. This is formula evaluation, not network simulation or a proof from slopes.

C: Relative mean broadcast prediction residuals from a fixed-ic CTMC calibrated at 32 nodes and evaluated without refitting up to 2,048. Independently simulated mesh/star/tree observations use per-node or global capacity. Each configuration has eight independent graph/seed clusters and twelve jitter trials per cluster; only meshes vary graph draws. Residuals are conditional fit errors, not universal deployment prediction errors.

D: Independent queued collection and strict SSF acknowledgment-path latencies, including every hierarchy level, root verification, explicit bitmap bytes, and proof dissemination. N=M, fixed per-node capacity, registered honest keys. Error bars are 95% cluster-bootstrap mean intervals; clock/execution and consensus-state validation are excluded. Total work is not equated to critical path.

E: Deadline-success estimates versus configured slot duration at N=M=2,048 for the strict SSF service path and sourced 3SF 5Delta service envelope. Shading conservatively encloses cluster-bootstrap and independent-draw Wilson intervals. These estimates concern service windows in an honest workload, not consensus safety or a complete implementation. Ninety-six trials/configuration cannot establish rare-failure reliability or a precise p99.

## Completed numerical results

The final sweep completed 75 CTMC configurations, 336 network configurations and 3,744 network trials in 48.56 seconds before dataset compression/plotting. It also completed 18 held-out prediction-sampling configurations (2,000 exact CTMC trajectories each). The 600-second stopping threshold was not reached. The largest independent network has 2,048 nodes; large-N moments reach one million identities.

Local calibration used blst 0.3.16 on an Intel Core i7-11800H in release mode, one thread. Mean signing/aggregate verification/merge-per-add/public-key-per-add costs were 320.33, 826.08, 1.181, and 0.448 microseconds. Variability and raw samples are retained; this warm local benchmark does not characterize Raspberry Pi/fat-node profiles or production tails.

Across 121 nonzero-SE mathematical mean checks, the largest absolute standardized discrepancy was 2.482; 12 lay outside their individual 95% intervals. These are correlated, paired parameter configurations, so that count is not a binomial goodness-of-fit test. Agreement is numerical consistency with the assumed rates, not validation of the rates. No individual discrepancy is a deterministic unit-test failure.

| Capacity at N=1,000,000 | Exact broadcast mean (s) | Exact aggregation mean (s) |
|---|---:|---:|
| fixed | 224889.712711 | 218552.618193 |
| sqrt | 1802.687094 | 1750.291592 |
| linear | 5.397267 | 3.634138 |

These values use lambda=1/s and D=8, and are analytical model outputs, not Ethereum predictions.

| Architecture/capacity, N=M=2048 | Independent broadcast (s) | Fixed-ic CTMC prediction (s) | Relative error | Independent aggregation (s) | CTMC aggregation (s) |
|---|---:|---:|---:|---:|---:|
| mesh/per_node | 0.076786 | 3.711982 | 4734.2% | 1.146703 | 3.516887 |
| mesh/global | 4.129771 | 5.292496 | 28.2% | 1.732160 | 5.014333 |
| central/per_node | 0.684783 | 2.185318 | 219.1% | 1.143345 | 2.070462 |
| central/global | 0.684783 | 2.185318 | 219.1% | 1.143351 | 2.070462 |
| hierarchy/per_node | 0.089963 | 3.917222 | 4254.3% | 0.096748 | 3.711340 |
| hierarchy/global | 0.702428 | 4.004610 | 470.1% | 0.136920 | 3.794136 |

The per-node mesh broadcast fit overpredicts by 48.34× after small-size calibration. The log-depth baseline predicts 0.090384s, an error of 17.7%, compared with 4734.2% for fixed ic. Under the shared bottleneck, the CTMC broadcast error is 28.2%; improved agreement is restricted to that imposed regime and still does not validate its exponential holding times. The independently simulated per-node mesh within-topology broadcast variance is 2.3362959e-06s² versus the fitted CTMC's 0.33426034s²: matching trend or mean would not establish matching variability.

| Architecture (per-node capacity) | Strict SSF path (s) | 3SF slot envelope (s) | 3SF scheduled finality (s) | Four-slot success at S/2 | Four-slot success at S |
|---|---:|---:|---:|---:|---:|
| mesh | 5.444484 | 4.473217 | 11.630365 | 0.000 | 0.250 |
| central | 6.045687 | 4.467411 | 11.615270 | 0.000 | 0.260 |
| hierarchy | 0.673344 | 0.485918 | 1.263388 | 0.000 | 0.156 |

The service-envelope estimates are based on isolated rounds; observed four-slot success shows why shared-resource validation is necessary. These conditional timing numbers exclude real execution and checkpoint/fork-choice handling. They support neither universal SSF infeasibility nor a guarantee that 3SF is exactly twice as fast.

The first central-collector trace has 1365 single-signer transitions before quorum. Its last 100 intercompletion gaps average 827.713 microseconds, with range [827.713, 827.713]. Once CPU service saturates, additional signer count does not accelerate these arrivals; this differs from the paper's j-dependent aggregation mechanism. This is a fixed-configuration diagnostic, not a pooled holding-time fit. Mesh traces also retain geometry-dependent receipt times; count-state sufficiency was not established.

| Fixed M=32 | Keys N | Broadcast mean (s) | Collection mean (s) |
|---|---:|---:|---:|
| mesh | 128 | 0.041084 | 0.082360 |
| mesh | 512 | 0.041084 | 0.288160 |
| mesh | 2048 | 0.041084 | 1.111384 |

Adding keys without adding nodes leaves proposal dissemination unchanged here and increases collection/signing work. This directly prevents interpreting key growth as growth in independently provisioned network resources or operators. Availability runs include A=96,N=128 (reachable global quorum) and A=80,N=128 (explicitly unreachable); their failures are retained, not filtered.

| Held-out per-node mesh at N=2048 | Network p50/p95/p99 (s) | CTMC p50/p95/p99 (s) | Network probability ≤1s | CTMC probability ≤1s |
|---|---|---|---:|---:|
| broadcast | 0.076151 / 0.083649 / 0.084530 | 3.629499 / 4.815376 / 5.449508 | 1.000 | 0.000 |
| aggregation | 1.146622 / 1.149446 / 1.149865 | 3.435650 / 4.576724 / 5.244658 | 0.000 | 0.000 |

Network probabilities include fixed-topology Wilson, independent-draw Wilson and cluster-bootstrap intervals in the dataset. A point estimate of zero or one is not a population certainty; p99 from 96 correlated-cluster observations is exploratory. Independent predicted CTMC deadline estimates have their own 2,000-trial binomial uncertainty.

## LaTeX experimental-section draft

The following draft uses only completed results. Figure references identify panels A–E of the supplied composition.

```latex
\section{Experimental verification and conditional validation}
We separated verification of the specified continuous-time Markov chains
from validation against an independently defined event-driven network.
The implementation used the manuscript transition rates, inverse-rate
moments, independently evaluated harmonic means, and exact exponential
holding-time samples. Quorums counted equal-weight keys, with
$Q=\lceil2N/3\rceil$; unreachable quorums were recorded explicitly.
Closed-form and inverse-rate means agreed in deterministic boundary and
representative tests. Exact moments covered $N\leq10^6$ under fixed,
square-root, and linear capacity growth. Sampling used three seeds and
6,000 trajectories per scaling configuration for $N\leq4096$
(Fig.~\ref{fig:companion}A--B).

The independent simulator used connected ring-plus-chord meshes, central
collection, and disjoint-subtree aggregation. It represented network nodes
separately from signing keys and charged serialized endpoint transfers,
propagation, queues, registered-key BLS component work, bitmap membership,
root verification, and certificate dissemination. Per-node and globally
shared upload resources were studied separately. Each baseline configuration
used eight independent topology or delay clusters and twelve trials per
cluster; only the mesh ensemble varied topology. Mean intervals used a
cluster bootstrap. Network p99 estimates are exploratory at this sample size.

The sweep completed 3744 network trials. With fixed $i_c=8$
and the product $\lambda D$ calibrated at $N=32$, the broadcast prediction
at $N=M=2048$ was 3.712 seconds, versus 0.077 seconds
in the per-node mesh, an overprediction of 48.3 times
(Fig.~\ref{fig:companion}C). Under an imposed shared bottleneck, the
corresponding relative prediction error was 28.2\%.
The log-depth baseline predicted 0.090 seconds for the per-node case.
Aggregation and variance also disagreed with the fitted CTMC. Thus numerical
verification of the mathematical model does not validate its network assumptions.

For architecture timing we used a sequential SSF acknowledgment path and
an aggregation-assisted schedule based on Algorithm 7 of
arXiv:2411.00558v1. With proposal service bounded by $\Delta$ and vote
service bounded by $2\Delta$, the modeled 3SF slot is $S=5\Delta$ and
finality observed at fast confirmation in slot $s+2$ occurs after
$2S+3\Delta$ from a proposal at the start of slot $s$.
Four concurrent slot workloads tested shared-resource contention
(Fig.~\ref{fig:companion}D--E). These are honest timing abstractions, not
implementations or proofs of checkpoint/fork-choice safety.

The results support capacity-dependent scaling within the chosen CTMC:
$\Theta(N\log N)$ for fixed effective parallelism and
$\Theta(\log N)$ for parallelism proportional to $N$.
They do not establish a protocol-independent lower bound, a universal
SSF/3SF speed ratio, or a decentralization/security theorem.
The exact expression must replace the fractional-quorum approximation at
$Q=A$, and an inequality for $E[L]$ alone does not imply the asserted upper
bound for $E[1/L]$. Future validation requires measured resource distributions,
arrival-order network service, execution workloads, adversarial behavior,
and protocol-specific state transitions.
```
