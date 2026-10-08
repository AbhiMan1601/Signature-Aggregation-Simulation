//! Scientific companion v1: exact paper CTMC, independent queued networks, and BLS calibration.
//! All times are seconds; validator identities have equal weight. See reports/paper-audit.md.
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeSet, BinaryHeap, VecDeque};
use std::io::{self, BufRead};
use std::time::Instant;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct Config {
    mode: String,
    n: usize,
    a: Option<usize>,
    online_fraction: Option<f64>,
    q: Option<usize>,
    lambda: f64,
    degree: f64,
    ic: f64,
    ic_a: Option<f64>,
    trials: usize,
    seed: u64,
    deadlines_s: Vec<f64>,
    m: usize,
    topology: String,
    topology_seed: u64,
    bandwidth_bytes_s: f64,
    global_bytes_s: Option<f64>,
    propagation_s: f64,
    jitter_s: f64,
    workers: usize,
    fanout: usize,
    proposal_bytes: usize,
    verify_s: f64,
    pk_s: f64,
    merge_s: f64,
    sign_s: f64,
    execution_s: f64,
    slot_s: f64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            mode: "ctmc".into(),
            n: 128,
            a: None,
            online_fraction: None,
            q: None,
            lambda: 1.,
            degree: 8.,
            ic: 8.,
            ic_a: None,
            trials: 1000,
            seed: 1,
            deadlines_s: vec![1., 12.],
            m: 128,
            topology: "mesh".into(),
            topology_seed: 1,
            bandwidth_bytes_s: 12_500_000.,
            global_bytes_s: None,
            propagation_s: 0.01,
            jitter_s: 0.005,
            workers: 1,
            fanout: 4,
            proposal_bytes: 4096,
            verify_s: 0.001,
            pk_s: 0.000001,
            merge_s: 0.000002,
            sign_s: 0.0005,
            execution_s: 0.,
            slot_s: 1.,
        }
    }
}
fn quorum(n: usize) -> usize {
    n - n / 3
}
impl Config {
    fn population(&self) -> Result<(usize, usize), String> {
        if self.n == 0 || self.n > 10_000_000 {
            return Err("N must be 1..10000000 (resource guard)".into());
        }
        if self.a.is_some() && self.online_fraction.is_some() {
            return Err("choose A or online_fraction".into());
        }
        let a = match self.online_fraction {
            Some(f) if f.is_finite() && f > 0. && f <= 1. => (f * self.n as f64).floor() as usize,
            Some(_) => return Err("online_fraction must be in (0,1]; A=floor(a*N)".into()),
            None => self.a.unwrap_or(self.n),
        };
        let q = self.q.unwrap_or_else(|| quorum(self.n));
        if a == 0 || a > self.n || q == 0 || q > self.n {
            return Err("require 1<=A,Q<=N".into());
        }
        for (name, x) in [
            ("lambda", self.lambda),
            ("degree", self.degree),
            ("ic", self.ic),
            ("ic_a", self.ic_a.unwrap_or(self.ic)),
        ] {
            if !x.is_finite() || x <= 0. {
                return Err(format!("{name} must be finite and positive"));
            }
        }
        if self.trials > 100_000 || self.deadlines_s.iter().any(|x| !x.is_finite() || *x < 0.) {
            return Err("invalid trials/deadline".into());
        }
        if self.mode == "ctmc" && (self.n as u128 * self.trials as u128) > 200_000_000 {
            return Err("exact sampling exceeds 200M-state guard; use trials=0 for moments".into());
        }
        Ok((a, q))
    }
    fn network_check(&self) -> Result<(), String> {
        self.population()?;
        if self.trials > 2000 || self.n as u128 * self.trials as u128 > 10_000_000 {
            return Err("network workload exceeds resource guard".into());
        }
        if self.m == 0
            || self.m > self.n
            || self.m > 8192
            || self.n > 16384
            || self.workers == 0
            || self.workers > 64
            || self.fanout < 2
        {
            return Err("invalid network size/workers/fanout".into());
        }
        for x in [self.bandwidth_bytes_s, self.slot_s] {
            if !x.is_finite() || x <= 0. {
                return Err("bandwidth/slot must be positive finite".into());
            }
        }
        if self
            .global_bytes_s
            .is_some_and(|x| !x.is_finite() || x <= 0.)
        {
            return Err("invalid global bandwidth".into());
        }
        for x in [
            self.propagation_s,
            self.jitter_s,
            self.verify_s,
            self.pk_s,
            self.merge_s,
            self.sign_s,
            self.execution_s,
        ] {
            if !x.is_finite() || x < 0. {
                return Err("negative/nonfinite network/CPU cost".into());
            }
        }
        if !["mesh", "central", "hierarchy", "chain"].contains(&self.topology.as_str()) {
            return Err("unknown topology".into());
        }
        Ok(())
    }
}
// Neumaier summation, independent of closed-form evaluation.
fn sum(xs: impl IntoIterator<Item = f64>) -> f64 {
    let (mut s, mut c) = (0_f64, 0_f64);
    for x in xs {
        let t = s + x;
        c += if s.abs() >= x.abs() {
            (s - t) + x
        } else {
            (x - t) + s
        };
        s = t;
    }
    s + c
}
fn harmonic(n: usize) -> f64 {
    sum((1..=n).map(|k| 1. / k as f64))
}
fn rates(c: &Config, a: usize, q: usize) -> (Vec<f64>, Vec<f64>) {
    let b = (1..c.n)
        .map(|i| {
            c.lambda * c.degree / (1. / i as f64 + 1. / c.ic)
                * ((c.n - i) as f64 / (c.n - 1) as f64)
        })
        .collect();
    let ag = if q > a {
        vec![]
    } else {
        (1..q)
            .map(|j| {
                c.lambda * c.degree / (1. / (a - j) as f64 + 1. / c.ic_a.unwrap_or(c.ic))
                    * (j as f64 / (a - 1) as f64)
            })
            .collect()
    };
    (b, ag)
}
#[derive(Serialize)]
struct Moments {
    mean_s: f64,
    variance_s2: f64,
    closed_mean_s: f64,
}
fn moments(r: &[f64], closed: f64) -> Result<Moments, String> {
    if r.iter().any(|x| !x.is_finite() || *x <= 0.) {
        return Err("rate overflow/underflow".into());
    }
    let mean = sum(r.iter().map(|x| 1. / x));
    let v = sum(r.iter().map(|x| (1. / x).powi(2)));
    if !mean.is_finite() || !v.is_finite() || !closed.is_finite() {
        return Err("moment overflow".into());
    }
    Ok(Moments {
        mean_s: mean,
        variance_s2: v,
        closed_mean_s: closed,
    })
}
fn closed(c: &Config, a: usize, q: usize) -> (f64, f64) {
    let b = (c.n - 1) as f64 / (c.lambda * c.degree)
        * harmonic(c.n - 1)
        * (2. / c.n as f64 + 1. / c.ic);
    let ag = if q > a {
        f64::INFINITY
    } else {
        (a - 1) as f64 / (c.lambda * c.degree)
            * ((harmonic(q - 1) + harmonic(a - 1) - harmonic(a - q)) / a as f64
                + harmonic(q - 1) / c.ic_a.unwrap_or(c.ic))
    };
    (b, ag)
}
fn sample(r: &[f64], rng: &mut ChaCha8Rng) -> f64 {
    sum(r.iter().map(|rate| -(-rng.gen::<f64>()).ln_1p() / rate))
}
#[derive(Serialize)]
struct Probability {
    deadline_s: f64,
    successes: usize,
    trials: usize,
    estimate: f64,
    ci95: [f64; 2],
}
fn wilson(k: usize, n: usize) -> [f64; 2] {
    if n == 0 {
        return [0., 1.];
    }
    let z = 1.959963984540054;
    let p = k as f64 / n as f64;
    let z2 = z * z / n as f64;
    let mid = (p + z2 / 2.) / (1. + z2);
    let half = z * ((p * (1. - p) / n as f64 + z2 / (4. * n as f64)).sqrt()) / (1. + z2);
    [(mid - half).max(0.), (mid + half).min(1.)]
}
#[derive(Serialize)]
struct Stats {
    trials: usize,
    mean_s: f64,
    variance_s2: f64,
    se_mean_s: f64,
    mean_ci95_s: [f64; 2],
    quantiles_s: [f64; 3],
    deadlines: Vec<Probability>,
}
fn stats(xs: &[f64], ds: &[f64]) -> Option<Stats> {
    if xs.is_empty() {
        return None;
    }
    let n = xs.len();
    let mean = sum(xs.iter().copied()) / n as f64;
    let v = if n > 1 {
        sum(xs.iter().map(|x| (x - mean).powi(2))) / (n - 1) as f64
    } else {
        0.
    };
    let se = (v / n as f64).sqrt();
    let mut sorted = xs.to_vec();
    sorted.sort_by(f64::total_cmp);
    Some(Stats {
        trials: n,
        mean_s: mean,
        variance_s2: v,
        se_mean_s: se,
        mean_ci95_s: [(mean - 1.96 * se).max(0.), mean + 1.96 * se],
        quantiles_s: [0.5, 0.95, 0.99].map(|p| sorted[((n - 1) as f64 * p).round() as usize]),
        deadlines: ds
            .iter()
            .map(|&d| {
                let k = xs.iter().filter(|&&x| x <= d).count();
                Probability {
                    deadline_s: d,
                    successes: k,
                    trials: n,
                    estimate: k as f64 / n as f64,
                    ci95: wilson(k, n),
                }
            })
            .collect(),
    })
}
#[derive(Serialize)]
struct CtmcOutput {
    schema: &'static str,
    config: Config,
    a: usize,
    q: usize,
    attainable: bool,
    broadcast: Moments,
    aggregation: Option<Moments>,
    mc_broadcast: Option<Stats>,
    mc_aggregation: Option<Stats>,
    raw_broadcast_s: Vec<f64>,
    raw_aggregation_s: Vec<f64>,
    runtime_s: f64,
}
fn ctmc(c: Config) -> Result<CtmcOutput, String> {
    let start = Instant::now();
    let (a, q) = c.population()?;
    let (b, ag) = rates(&c, a, q);
    let (cb, ca) = closed(&c, a, q);
    let broadcast = moments(&b, cb)?;
    let aggregation = if q <= a {
        Some(moments(&ag, ca)?)
    } else {
        None
    };
    let mut rb = ChaCha8Rng::seed_from_u64(c.seed);
    let mut ra = ChaCha8Rng::seed_from_u64(c.seed ^ 0xa0761d6478bd642f);
    let xb = (0..c.trials)
        .map(|_| sample(&b, &mut rb))
        .collect::<Vec<_>>();
    let xa = if q <= a {
        (0..c.trials)
            .map(|_| sample(&ag, &mut ra))
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    Ok(CtmcOutput {
        schema: "paper-v1",
        mc_broadcast: stats(&xb, &c.deadlines_s),
        mc_aggregation: stats(&xa, &c.deadlines_s),
        config: c,
        a,
        q,
        attainable: q <= a,
        broadcast,
        aggregation,
        raw_broadcast_s: xb,
        raw_aggregation_s: xa,
        runtime_s: start.elapsed().as_secs_f64(),
    })
}

#[derive(Clone, Debug)]
enum Kind {
    Broadcast {
        message: usize,
        from: usize,
        to: usize,
        bytes: usize,
        proof: bool,
    },
    Own {
        round: usize,
        node: usize,
    },
    Vote {
        round: usize,
        node: usize,
        signers: BTreeSet<usize>,
    },
    Ready {
        round: usize,
        node: usize,
        signers: BTreeSet<usize>,
    },
}
#[derive(Clone, Debug)]
struct Event {
    at: f64,
    id: usize,
    kind: Kind,
}
impl PartialEq for Event {
    fn eq(&self, o: &Self) -> bool {
        self.at == o.at && self.id == o.id
    }
}
impl Eq for Event {}
impl PartialOrd for Event {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Event {
    fn cmp(&self, o: &Self) -> Ordering {
        self.at.total_cmp(&o.at).then(self.id.cmp(&o.id))
    }
}
#[derive(Clone, Default, Debug, Serialize)]
struct Metrics {
    bytes_sent: u64,
    bytes_received: u64,
    duplicate_bytes: u64,
    upload_queue_s: f64,
    download_queue_s: f64,
    cpu_queue_s: f64,
    cpu_busy_s: f64,
    messages: usize,
    max_signers: usize,
}
struct Engine<'a> {
    c: &'a Config,
    graph: Vec<Vec<usize>>,
    parent: Vec<usize>,
    keys: Vec<BTreeSet<usize>>,
    online: Vec<bool>,
    up: Vec<f64>,
    down: Vec<f64>,
    cpu: Vec<Vec<f64>>,
    global: f64,
    events: BinaryHeap<Reverse<Event>>,
    id: usize,
    rng: ChaCha8Rng,
    metrics: Metrics,
}
impl<'a> Engine<'a> {
    fn new(c: &'a Config, seed: u64) -> Result<Self, String> {
        c.network_check()?;
        let (a, _) = c.population()?;
        let mut keys = vec![BTreeSet::new(); c.m];
        for k in 0..a {
            keys[k % c.m].insert(k);
        }
        let online = keys.iter().map(|x| !x.is_empty()).collect::<Vec<_>>();
        let nodes = (0..c.m).filter(|&i| online[i]).collect::<Vec<_>>();
        let mut graph = vec![Vec::new(); c.m];
        fn edge(g: &mut [Vec<usize>], u: usize, v: usize) {
            if u != v && !g[u].contains(&v) {
                g[u].push(v);
                g[v].push(u);
            }
        }
        for i in 1..nodes.len() {
            let p = match c.topology.as_str() {
                "central" => 0,
                "hierarchy" => (i - 1) / c.fanout,
                _ => i - 1,
            };
            edge(&mut graph, nodes[p], nodes[i]);
        }
        if c.topology == "mesh" && nodes.len() > 1 {
            edge(&mut graph, 0, *nodes.last().unwrap());
            let mut rng = ChaCha8Rng::seed_from_u64(c.topology_seed);
            let target = ((c.degree * nodes.len() as f64 / 2.).round() as usize)
                .min(nodes.len() * (nodes.len() - 1) / 2);
            // Ring backbone guarantees connectivity; randomized chords, bounded attempts.
            let mut edges = graph.iter().map(Vec::len).sum::<usize>() / 2;
            for _ in 0..target * 20 {
                if edges >= target {
                    break;
                }
                let u = nodes[rng.gen_range(0..nodes.len())];
                let v = nodes[rng.gen_range(0..nodes.len())];
                if u != v && !graph[u].contains(&v) {
                    edge(&mut graph, u, v);
                    edges += 1;
                }
            }
        }
        for ns in &mut graph {
            ns.sort_unstable();
        }
        let mut parent = vec![usize::MAX; c.m];
        parent[0] = 0;
        let mut queue = VecDeque::from([0]);
        while let Some(u) = queue.pop_front() {
            for &v in &graph[u] {
                if parent[v] == usize::MAX {
                    parent[v] = u;
                    queue.push_back(v);
                }
            }
        }
        Ok(Self {
            c,
            graph,
            parent,
            keys,
            online,
            up: vec![0.; c.m],
            down: vec![0.; c.m],
            cpu: vec![vec![0.; c.workers]; c.m],
            global: 0.,
            events: BinaryHeap::new(),
            id: 0,
            rng: ChaCha8Rng::seed_from_u64(seed),
            metrics: Metrics::default(),
        })
    }
    fn push(&mut self, at: f64, kind: Kind) {
        self.id += 1;
        self.events.push(Reverse(Event {
            at,
            id: self.id,
            kind,
        }));
    }
    fn cpu(&mut self, node: usize, at: f64, cost: f64) -> f64 {
        let w = (0..self.c.workers)
            .min_by(|&i, &j| self.cpu[node][i].total_cmp(&self.cpu[node][j]))
            .unwrap();
        let begin = at.max(self.cpu[node][w]);
        self.metrics.cpu_queue_s += begin - at;
        self.metrics.cpu_busy_s += cost;
        self.cpu[node][w] = begin + cost;
        begin + cost
    }
    fn transfer(&mut self, from: usize, to: usize, at: f64, bytes: usize) -> f64 {
        let mut begin = at.max(self.up[from]);
        if self.c.global_bytes_s.is_some() {
            begin = begin.max(self.global);
        }
        self.metrics.upload_queue_s += begin - at;
        let serialization = bytes as f64 / self.c.bandwidth_bytes_s;
        self.up[from] = begin + serialization;
        if let Some(b) = self.c.global_bytes_s {
            self.global = begin + bytes as f64 / b;
            self.up[from] = self.up[from].max(self.global);
        }
        let arrive = self.up[from] + self.c.propagation_s + self.rng.gen::<f64>() * self.c.jitter_s;
        let receive = arrive.max(self.down[to]);
        self.metrics.download_queue_s += receive - arrive;
        self.down[to] = receive + serialization;
        self.metrics.bytes_sent += bytes as u64;
        self.metrics.bytes_received += bytes as u64;
        self.metrics.messages += 1;
        self.down[to]
    }
    fn broadcast(&mut self, message: usize, from: usize, at: f64, bytes: usize, proof: bool) {
        self.push(
            at,
            Kind::Broadcast {
                message,
                from,
                to: from,
                bytes,
                proof,
            },
        );
    }
    fn certificate_bytes(&self) -> usize {
        96 + self.c.n.div_ceil(8) + 16
    }
    fn run(&mut self, start: f64, rounds: usize, proposal: bool) -> RoundOutput {
        self.run_many(start, rounds, &[start], proposal, None)
    }
    fn run_many(
        &mut self,
        start: f64,
        phases: usize,
        starts: &[f64],
        proposal: bool,
        vote_offset_s: Option<f64>,
    ) -> RoundOutput {
        let rounds = phases * starts.len();
        let (_, q) = self.c.population().unwrap();
        let a = self.keys.iter().map(BTreeSet::len).sum::<usize>();
        if q > a {
            return RoundOutput {
                attainable: false,
                ..Default::default()
            };
        }
        let mut seen = vec![vec![false; self.c.m]; (phases + 1) * starts.len()];
        let mut counts = vec![0; (phases + 1) * starts.len()];
        let mut broadcast_s = 0_f64;
        let mut collection_s = vec![0.; rounds];
        let mut proof_end = vec![0_f64; rounds];
        let mut coverage = vec![vec![BTreeSet::new(); self.c.m]; rounds];
        let mut received = vec![vec![0_usize; self.c.m]; rounds];
        let mut own = vec![vec![false; self.c.m]; rounds];
        let mut sent = vec![vec![false; self.c.m]; rounds];
        let mut certified = vec![false; rounds];
        let mut certificate_members = vec![0_usize; rounds];
        let children = (0..self.c.m)
            .map(|u| {
                (1..self.c.m)
                    .filter(|&v| self.online[v] && self.parent[v] == u)
                    .count()
            })
            .collect::<Vec<_>>();
        let mut launched = vec![false; rounds];
        for (block, &at) in starts.iter().enumerate() {
            if proposal {
                self.broadcast(block * (phases + 1), 0, at, self.c.proposal_bytes, false);
            } else {
                launched[block * phases] = true;
                self.launch(block * phases, at);
            }
        }
        let mut proposal_at = vec![0_f64; starts.len()];
        let mut end = start;
        let mut trace = Vec::new();
        let mut aggregation_trace = Vec::new();
        while let Some(Reverse(e)) = self.events.pop() {
            end = end.max(e.at);
            let touched = match &e.kind {
                Kind::Own { round, node } | Kind::Ready { round, node, .. } => {
                    Some((*node, *round))
                }
                _ => None,
            };
            match e.kind {
                Kind::Broadcast {
                    message,
                    from,
                    to,
                    bytes,
                    proof,
                } => {
                    let block = message / (phases + 1);
                    let phase = message % (phases + 1);
                    let index = block * phases + phase.saturating_sub(1);
                    if seen[message][to] {
                        self.metrics.duplicate_bytes += bytes as u64;
                        continue;
                    }
                    seen[message][to] = true;
                    counts[message] += 1;
                    let ready = if proof {
                        self.cpu(
                            to,
                            e.at,
                            self.c.verify_s + certificate_members[index] as f64 * self.c.pk_s,
                        )
                    } else {
                        self.cpu(to, e.at, self.c.execution_s)
                    };
                    // All-node receipt metric; verification completes at ready, forwarding follows processing.
                    if phase == 0 {
                        broadcast_s = broadcast_s.max(ready - starts[block]);
                        proposal_at[block] = proposal_at[block].max(ready);
                        trace.push((counts[message], ready - start));
                    } else {
                        proof_end[index] = proof_end[index].max(ready);
                    }
                    for v in self.graph[to].clone() {
                        if v != from {
                            let t = self.transfer(to, v, ready, bytes);
                            self.push(
                                t,
                                Kind::Broadcast {
                                    message,
                                    from: to,
                                    to: v,
                                    bytes,
                                    proof,
                                },
                            );
                        }
                    }
                    if counts[message] == self.online.iter().filter(|&&x| x).count() {
                        let next = block * phases + phase;
                        if phase < phases && !launched[next] {
                            launched[next] = true;
                            self.launch(
                                next,
                                if phase == 0 {
                                    proposal_at[block]
                                        .max(starts[block] + vote_offset_s.unwrap_or(0.))
                                } else {
                                    proof_end[index]
                                },
                            );
                        }
                    }
                }
                Kind::Own { round, node } => {
                    own[round][node] = true;
                    if self.c.topology == "hierarchy" {
                        assert!(disjoint_union(&mut coverage[round][node], &self.keys[node]));
                    } else {
                        for k in self.keys[node].clone() {
                            let signers = BTreeSet::from([k]);
                            if node == 0 {
                                self.push(
                                    e.at,
                                    Kind::Ready {
                                        round,
                                        node,
                                        signers,
                                    },
                                );
                            } else {
                                let p = self.parent[node];
                                let t = self.transfer(node, p, e.at, 112);
                                self.push(
                                    t,
                                    Kind::Vote {
                                        round,
                                        node: p,
                                        signers,
                                    },
                                );
                            }
                        }
                    }
                }
                Kind::Vote {
                    round,
                    node,
                    signers,
                } => {
                    let hierarchical = self.c.topology == "hierarchy";
                    if !hierarchical && node != 0 {
                        let t = self.transfer(node, self.parent[node], e.at, 112);
                        self.push(
                            t,
                            Kind::Vote {
                                round,
                                node: self.parent[node],
                                signers,
                            },
                        );
                        continue;
                    }
                    let cost =
                        self.c.verify_s + signers.len() as f64 * self.c.pk_s + self.c.merge_s;
                    let t = self.cpu(node, e.at, cost);
                    self.push(
                        t,
                        Kind::Ready {
                            round,
                            node,
                            signers,
                        },
                    );
                    continue;
                }
                Kind::Ready {
                    round,
                    node,
                    signers,
                } => {
                    if self.c.topology == "hierarchy" {
                        assert!(disjoint_union(&mut coverage[round][node], &signers));
                        received[round][node] += 1;
                    } else {
                        coverage[round][0].extend(signers);
                    }
                    if node == 0 && !certified[round] {
                        aggregation_trace.push((round, coverage[round][0].len(), e.at));
                    }
                    if node == 0 && !certified[round] && coverage[round][0].len() >= q {
                        certified[round] = true;
                        certificate_members[round] = coverage[round][0].len();
                        collection_s[round] = e.at;
                        self.metrics.max_signers =
                            self.metrics.max_signers.max(coverage[round][0].len());
                        let t = self.cpu(
                            0,
                            e.at,
                            self.c.verify_s + coverage[round][0].len() as f64 * self.c.pk_s,
                        );
                        self.broadcast(
                            round / phases * (phases + 1) + round % phases + 1,
                            0,
                            t,
                            self.certificate_bytes(),
                            true,
                        );
                    }
                }
            }
            // Subtree batches are emitted once, only after all online child batches and own keys.
            if self.c.topology == "hierarchy" {
                if let Some((node, round)) = touched {
                    if own[round][node]
                        && received[round][node] == children[node]
                        && !sent[round][node]
                    {
                        sent[round][node] = true;
                        if node == 0 && !certified[round] && coverage[round][node].len() >= q {
                            certified[round] = true;
                            certificate_members[round] = coverage[round][node].len();
                            collection_s[round] = end;
                            self.metrics.max_signers =
                                self.metrics.max_signers.max(coverage[round][node].len());
                            let t = self.cpu(
                                0,
                                end,
                                self.c.verify_s + coverage[round][node].len() as f64 * self.c.pk_s,
                            );
                            self.broadcast(
                                round / phases * (phases + 1) + round % phases + 1,
                                0,
                                t,
                                self.certificate_bytes(),
                                true,
                            );
                        } else if node != 0 {
                            let members = coverage[round][node].clone();
                            let t = self.cpu(node, end, self.c.merge_s * members.len() as f64);
                            let t =
                                self.transfer(node, self.parent[node], t, self.certificate_bytes());
                            self.push(
                                t,
                                Kind::Vote {
                                    round,
                                    node: self.parent[node],
                                    signers: members,
                                },
                            );
                        }
                    }
                }
            }
        }
        RoundOutput {
            attainable: certified.iter().all(|&v| v),
            broadcast_s,
            collection_at_s: collection_s,
            proof_at_s: proof_end.clone(),
            completion_s: end.max(proof_end.iter().copied().fold(start, f64::max)) - start,
            broadcast_trace: trace,
            aggregation_trace,
        }
    }
    fn launch(&mut self, round: usize, at: f64) {
        for node in 0..self.c.m {
            if self.online[node] {
                let t = self.cpu(node, at, self.keys[node].len() as f64 * self.c.sign_s);
                self.push(t, Kind::Own { round, node });
            }
        }
    }
}
fn disjoint_union(a: &mut BTreeSet<usize>, b: &BTreeSet<usize>) -> bool {
    if !a.is_disjoint(b) {
        return false;
    }
    a.extend(b);
    true
}
#[derive(Default, Serialize)]
struct RoundOutput {
    attainable: bool,
    broadcast_s: f64,
    collection_at_s: Vec<f64>,
    proof_at_s: Vec<f64>,
    completion_s: f64,
    broadcast_trace: Vec<(usize, f64)>,
    aggregation_trace: Vec<(usize, usize, f64)>,
}
#[derive(Serialize)]
struct NetworkTrial {
    seed: u64,
    proposal_s: f64,
    aggregation_s: f64,
    proof_s: f64,
    ssf_required_slot_s: f64,
    three_sf_required_slot_s: f64,
    three_sf_finality_s: f64,
    pipeline: Vec<PipelineResult>,
    metrics: Metrics,
    cpu_utilization: f64,
    trace: Vec<(usize, f64)>,
    aggregation_trace: Vec<(usize, usize, f64)>,
}
#[derive(Serialize)]
struct PipelineResult {
    slot_s: f64,
    slots: usize,
    success: bool,
    completion_at_s: Vec<f64>,
    metrics: Metrics,
}
fn network(c: Config) -> Result<impl Serialize, String> {
    c.network_check()?;
    let start = Instant::now();
    let (a, q) = c.population()?;
    let mut trials = Vec::new();
    if q <= a {
        for i in 0..c.trials {
            let seed = c.seed.wrapping_add(i as u64);
            let mut e = Engine::new(&c, seed)?;
            let output = e.run(0., 1, true);
            if !output.attainable {
                return Err("network failed quorum despite online connected graph".into());
            }
            let proposal = output.broadcast_s;
            let collection = output.collection_at_s[0] - proposal;
            let proof = output.proof_at_s[0] - output.collection_at_s[0];
            // Resource-real SSF: proposal then head, FFG, acknowledgment certificates, sequential dependencies.
            let mut ssf = Engine::new(&c, seed)?;
            let ssf_out = ssf.run(0., 3, true);
            // Source Algorithm 7: 4 Delta rounds (5 Delta with two-step aggregated vote delivery).
            // Delta is a common service envelope: proposal <=Delta and vote delivery<=2Delta.
            let delta = proposal.max((collection + proof) / 2.);
            let slot = 5. * delta;
            // Finality observed at fixed fast-confirm in slot s+2: 2*slot + 3*Delta.
            let mut pipeline = Vec::new();
            for fraction in [0.5, 1.] {
                let slot_s = slot * fraction;
                let starts = (0..4).map(|k| k as f64 * slot_s).collect::<Vec<_>>();
                let mut engine = Engine::new(&c, seed)?;
                let run = engine.run_many(0., 1, &starts, true, Some(slot_s / 5.));
                let success = run.attainable
                    && run
                        .proof_at_s
                        .iter()
                        .zip(&starts)
                        .all(|(&at, &s)| at <= s + 3. * slot_s / 5.);
                pipeline.push(PipelineResult {
                    slot_s,
                    slots: 4,
                    success,
                    completion_at_s: run.proof_at_s,
                    metrics: engine.metrics,
                });
            }
            trials.push(NetworkTrial {
                pipeline,
                seed,
                proposal_s: proposal,
                aggregation_s: collection,
                proof_s: proof,
                ssf_required_slot_s: ssf_out.proof_at_s[2],
                three_sf_required_slot_s: slot,
                three_sf_finality_s: 2. * slot + 3. * delta,
                cpu_utilization: e.metrics.cpu_busy_s
                    / (output.completion_s.max(1e-12) * c.m as f64 * c.workers as f64),
                metrics: e.metrics,
                trace: output.broadcast_trace,
                aggregation_trace: output.aggregation_trace,
            });
        }
    }
    #[derive(Serialize)]
    struct Output {
        schema: &'static str,
        config: Config,
        a: usize,
        q: usize,
        attainable: bool,
        raw: Vec<NetworkTrial>,
        runtime_s: f64,
    }
    Ok(Output {
        schema: "network-v1",
        config: c,
        a,
        q,
        attainable: q <= a,
        raw: trials,
        runtime_s: start.elapsed().as_secs_f64(),
    })
}
fn bls_bench(c: &Config) -> Result<impl Serialize, String> {
    use blst::min_pk::{AggregatePublicKey, AggregateSignature, SecretKey};
    let dst = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";
    let msg = b"paper companion benchmark";
    if c.trials < 2 || c.trials > 10000 {
        return Err("BLS trials must be 2..10000".into());
    }
    let mut rng = ChaCha8Rng::seed_from_u64(c.seed);
    let mut keys = Vec::new();
    for _ in 0..32 {
        let mut ikm = [0u8; 32];
        rng.fill(&mut ikm);
        keys.push(SecretKey::key_gen(&ikm, &[]).map_err(|e| format!("{e:?}"))?);
    }
    let pubs = keys.iter().map(|k| k.sk_to_pk()).collect::<Vec<_>>();
    let sigs = keys
        .iter()
        .map(|k| k.sign(msg, dst, &[]))
        .collect::<Vec<_>>();
    let pr = pubs.iter().collect::<Vec<_>>();
    let sr = sigs.iter().collect::<Vec<_>>();
    let pk = AggregatePublicKey::aggregate(&pr, true)
        .unwrap()
        .to_public_key();
    let sg = AggregateSignature::aggregate(&sr, true)
        .unwrap()
        .to_signature();
    let (mut sign, mut verify, mut merge, mut pks) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for i in 0..c.trials {
        let t = Instant::now();
        std::hint::black_box(keys[i % 32].sign(msg, dst, &[]));
        sign.push(t.elapsed().as_secs_f64());
        let t = Instant::now();
        assert_eq!(
            sg.verify(true, msg, dst, &[], &pk, true),
            blst::BLST_ERROR::BLST_SUCCESS
        );
        verify.push(t.elapsed().as_secs_f64());
        let t = Instant::now();
        std::hint::black_box(AggregateSignature::aggregate(&sr, false).unwrap());
        merge.push(t.elapsed().as_secs_f64() / 31.);
        let t = Instant::now();
        std::hint::black_box(AggregatePublicKey::aggregate(&pr, false).unwrap());
        pks.push(t.elapsed().as_secs_f64() / 31.);
    }
    #[derive(Serialize)]
    struct Bench {
        schema: &'static str,
        config: Config,
        sign: Stats,
        verify: Stats,
        merge_per_add: Stats,
        pk_per_add: Stats,
        raw: Vec<Vec<f64>>,
    }
    Ok(Bench {
        schema: "bls-v1",
        config: c.clone(),
        sign: stats(&sign, &[]).unwrap(),
        verify: stats(&verify, &[]).unwrap(),
        merge_per_add: stats(&merge, &[]).unwrap(),
        pk_per_add: stats(&pks, &[]).unwrap(),
        raw: vec![sign, verify, merge, pks],
    })
}
fn dispatch(c: Config) -> Result<String, String> {
    match c.mode.as_str() {
        "ctmc" => serde_json_string(&ctmc(c)?),
        "network" => serde_json_string(&network(c)?),
        "bls" => serde_json_string(&bls_bench(&c)?),
        _ => Err("mode must be ctmc/network/bls".into()),
    }
}
fn serde_json_string<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}
fn main() {
    // JSON Lines batch CLI: one fully validated configuration per line, one result per line.
    // Example: echo '{"n":128,"trials":1000}' | cargo run --release --bin paper_ctmc
    for line in io::stdin().lock().lines() {
        let result = line
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<Config>(&s).map_err(|e| e.to_string()))
            .and_then(dispatch);
        match result {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("ERROR: {e}");
                std::process::exit(2);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_closed_forms() {
        for n in [1_usize, 2, 3, 10, 127, 1024] {
            for a in [1, n.div_ceil(2), n] {
                for q in [1, a] {
                    for ic in [0.5, 8., n as f64] {
                        let c = Config {
                            n,
                            a: Some(a),
                            q: Some(q),
                            ic,
                            ..Config::default()
                        };
                        let (b, ag) = rates(&c, a, q);
                        let (cb, ca) = closed(&c, a, q);
                        for (xs, v) in [(b, cb), (ag, ca)] {
                            let m = moments(&xs, v).unwrap();
                            assert!((m.mean_s - v).abs() < 1e-11 * (1. + v));
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn two_node_distribution() {
        let c = Config {
            n: 2,
            a: Some(2),
            q: Some(2),
            lambda: 2.,
            degree: 3.,
            ic: 1.,
            ..Config::default()
        };
        let (b, a) = rates(&c, 2, 2);
        assert_eq!(b, vec![3.]);
        assert_eq!(a, vec![3.]);
        assert_eq!(moments(&b, 1. / 3.).unwrap().variance_s2, 1. / 9.);
    }
    #[test]
    fn boundaries() {
        let c = Config {
            n: 3,
            a: Some(1),
            ..Config::default()
        };
        let o = ctmc(c).unwrap();
        assert!(!o.attainable);
        assert!(o.aggregation.is_none());
        let c = Config {
            n: 1,
            m: 1,
            q: Some(1),
            trials: 2,
            ..Config::default()
        };
        let o = ctmc(c).unwrap();
        assert_eq!(o.raw_aggregation_s, vec![0., 0.]);
    }
    #[test]
    fn invalid_inputs() {
        for c in [
            Config {
                lambda: 0.,
                ..Config::default()
            },
            Config {
                ic: f64::NAN,
                ..Config::default()
            },
            Config {
                a: Some(129),
                ..Config::default()
            },
            Config {
                online_fraction: Some(0.),
                ..Config::default()
            },
        ] {
            assert!(ctmc(c).is_err());
        }
    }
    #[test]
    fn queue_hand_calculation() {
        let c = Config {
            n: 2,
            m: 2,
            bandwidth_bytes_s: 100.,
            propagation_s: 0.5,
            jitter_s: 0.,
            ..Config::default()
        };
        let mut e = Engine::new(&c, 1).unwrap();
        assert_eq!(e.transfer(0, 1, 0., 100), 2.5);
        assert_eq!(e.transfer(0, 1, 0., 100), 3.5);
        assert_eq!(e.metrics.upload_queue_s, 1.);
        assert_eq!(e.cpu(0, 0., 2.), 2.);
        assert_eq!(e.cpu(0, 1., 1.), 3.);
    }
    #[test]
    fn small_broadcast_and_quorum() {
        let c = Config {
            n: 2,
            m: 2,
            bandwidth_bytes_s: 100.,
            proposal_bytes: 100,
            propagation_s: 0.5,
            jitter_s: 0.,
            sign_s: 0.,
            verify_s: 0.,
            pk_s: 0.,
            merge_s: 0.,
            ..Config::default()
        };
        let mut e = Engine::new(&c, 1).unwrap();
        let o = e.run(0., 1, true);
        assert_eq!(o.broadcast_s, 2.5);
        assert!(o.attainable);
        assert_eq!(e.metrics.max_signers, 2);
    }
    #[test]
    fn hierarchy_disjoint_coverage() {
        for n in [1, 2, 7, 13] {
            let c = Config {
                n,
                m: n,
                q: Some(n),
                topology: "hierarchy".into(),
                ..Config::default()
            };
            let mut e = Engine::new(&c, 1).unwrap();
            let o = e.run(0., 1, true);
            assert!(o.attainable);
            assert_eq!(e.metrics.max_signers, n);
        }
    }
    #[test]
    fn duplicate_signer_rejected() {
        let mut a = BTreeSet::from([1, 2]);
        assert!(!disjoint_union(&mut a, &BTreeSet::from([2, 3])));
        assert_eq!(a.len(), 2);
        assert!(disjoint_union(&mut a, &BTreeSet::from([3])));
    }
    #[test]
    fn replay() {
        let c = Config {
            n: 16,
            m: 16,
            ..Config::default()
        };
        let mut e = Engine::new(&c, 3).unwrap();
        let x = e.run(0., 3, true);
        let mut e = Engine::new(&c, 3).unwrap();
        let y = e.run(0., 3, true);
        assert_eq!(x.proof_at_s, y.proof_at_s);
        assert_eq!(x.broadcast_trace, y.broadcast_trace);
    }
    #[test]
    fn overlapping_slots_share_resources() {
        let c = Config {
            n: 16,
            m: 16,
            topology: "central".into(),
            ..Config::default()
        };
        let starts = [0., 0.0001, 0.0002, 0.0003];
        let mut e = Engine::new(&c, 3).unwrap();
        let x = e.run_many(0., 1, &starts, true, None);
        assert!(x.attainable);
        assert_eq!(x.proof_at_s.len(), 4);
        assert!(e.metrics.cpu_queue_s > 0.);
        let mut e = Engine::new(&c, 3).unwrap();
        let y = e.run_many(0., 1, &starts, true, None);
        assert_eq!(x.proof_at_s, y.proof_at_s);
    }
    #[test]
    fn certificate_bls_and_invalid_message() {
        use blst::min_pk::{AggregatePublicKey, AggregateSignature, SecretKey};
        let dst = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";
        let sks = (1..=3)
            .map(|i| SecretKey::key_gen(&[i; 32], &[]).unwrap())
            .collect::<Vec<_>>();
        let p = sks.iter().map(|s| s.sk_to_pk()).collect::<Vec<_>>();
        let s = sks
            .iter()
            .map(|s| s.sign(b"vote", dst, &[]))
            .collect::<Vec<_>>();
        let pk = AggregatePublicKey::aggregate(&p.iter().collect::<Vec<_>>(), true)
            .unwrap()
            .to_public_key();
        let sig = AggregateSignature::aggregate(&s.iter().collect::<Vec<_>>(), true)
            .unwrap()
            .to_signature();
        assert_eq!(
            sig.verify(true, b"vote", dst, &[], &pk, true),
            blst::BLST_ERROR::BLST_SUCCESS
        );
        assert_ne!(
            sig.verify(true, b"invalid", dst, &[], &pk, true),
            blst::BLST_ERROR::BLST_SUCCESS
        );
    }
}
