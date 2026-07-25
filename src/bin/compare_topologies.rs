use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::env;

const VERIFY_MS: f64 = 882.21440825 / 1_000_000.0 * 1000.0;
const AGG_SIG_MS: f64 = 1.068981688 / 1_000_000.0 * 1000.0;
const AGG_PUB_MS: f64 = 0.460724574 / 1_000_000.0 * 1000.0;
const SLOT_MS: f64 = 12_000.0;
const MIN_QUORUM: f64 = 2.0 / 3.0;

#[derive(Debug, Clone)]
struct Options {
    validators: Vec<usize>,
    trials: usize,
    seed: u64,
    csv: bool,
}

#[derive(Debug, Clone)]
struct SampledParams {
    quorum: f64,
    bandwidth_mbps: f64,
    evidence_bytes_per_validator: f64,
    raw_vote_bytes: f64,
    hop_ms: f64,
    duplicate_factor: f64,
    crypto_multiplier: f64,
    cores: f64,
}

#[derive(Debug, Clone)]
struct TrialResult {
    topology: &'static str,
    avg_degree: Option<f64>,
    diameter_hops: Option<f64>,
    broadcast_ms: f64,
    signature_aggregation_ms: f64,
    round_ms: f64,
}

#[derive(Debug, Clone)]
struct Summary {
    validators: usize,
    topology: &'static str,
    p50_avg_degree: Option<f64>,
    p50_diameter_hops: Option<f64>,
    p50_broadcast_ms: f64,
    p50_signature_aggregation_ms: f64,
    p50_round_ms: f64,
    p90_round_ms: f64,
    p99_round_ms: f64,
    ssf_fit_pct: f64,
    three_sf_fit_pct: f64,
}

fn main() {
    let opts = parse_args();
    let summaries = run_monte_carlo(&opts);

    if opts.csv {
        print_csv(&summaries);
    } else {
        print_table(&summaries, &opts);
    }
}

fn parse_args() -> Options {
    let mut opts = Options {
        validators: vec![10, 100, 1_000, 10_000, 100_000, 1_000_000],
        trials: 2_000,
        seed: 42,
        csv: false,
    };

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--validators" => {
                if let Some(value) = args.next() {
                    opts.validators = value
                        .split(',')
                        .filter_map(|item| item.replace('_', "").parse::<usize>().ok())
                        .collect();
                }
            }
            "--trials" => {
                if let Some(value) = args.next() {
                    opts.trials = value.parse::<usize>().unwrap_or(opts.trials).max(1);
                }
            }
            "--seed" => {
                if let Some(value) = args.next() {
                    opts.seed = value.parse::<u64>().unwrap_or(opts.seed);
                }
            }
            "--csv" => opts.csv = true,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => eprintln!("Ignoring unknown argument: {other}"),
        }
    }

    opts
}

fn print_help() {
    println!("Usage: compare_topologies [--csv] [--validators 10,100,1000,10000,100000,1000000] [--trials 2000] [--seed 42]");
    println!("Runs a Monte Carlo sensitivity model for standalone SSF and 3SF aggregation timing.");
    println!("Each topology round is max(evidence lower bound, propagation cost, topology-specific collection/crypto cost).");
    println!("SSF fits if 2 * round_ms <= 12000. 3SF slot fits if round_ms <= 12000.");
}

fn run_monte_carlo(opts: &Options) -> Vec<Summary> {
    let mut summaries = Vec::new();

    for &validators in &opts.validators {
        let mut per_topology = topology_names()
            .iter()
            .map(|&name| (name, Vec::with_capacity(opts.trials)))
            .collect::<Vec<_>>();

        for trial in 0..opts.trials {
            let mut rng =
                StdRng::seed_from_u64(opts.seed ^ ((validators as u64) << 16) ^ trial as u64);
            let params = sample_params(&mut rng);
            let results = evaluate_trial(validators, &params);

            for result in results {
                if let Some((_, values)) = per_topology
                    .iter_mut()
                    .find(|(name, _)| *name == result.topology)
                {
                    values.push(result);
                }
            }
        }

        for (topology, values) in per_topology {
            let mut degrees = values
                .iter()
                .filter_map(|value| value.avg_degree)
                .collect::<Vec<_>>();
            let mut diameters = values
                .iter()
                .filter_map(|value| value.diameter_hops)
                .collect::<Vec<_>>();
            let mut broadcasts = values
                .iter()
                .map(|value| value.broadcast_ms)
                .collect::<Vec<_>>();
            let mut aggregations = values
                .iter()
                .map(|value| value.signature_aggregation_ms)
                .collect::<Vec<_>>();
            let mut rounds = values
                .iter()
                .map(|value| value.round_ms)
                .collect::<Vec<_>>();

            sort_f64(&mut degrees);
            sort_f64(&mut diameters);
            sort_f64(&mut broadcasts);
            sort_f64(&mut aggregations);
            sort_f64(&mut rounds);

            summaries.push(Summary {
                validators,
                topology,
                p50_avg_degree: optional_percentile(&degrees, 0.50),
                p50_diameter_hops: optional_percentile(&diameters, 0.50),
                p50_broadcast_ms: percentile(&broadcasts, 0.50),
                p50_signature_aggregation_ms: percentile(&aggregations, 0.50),
                p50_round_ms: percentile(&rounds, 0.50),
                p90_round_ms: percentile(&rounds, 0.90),
                p99_round_ms: percentile(&rounds, 0.99),
                ssf_fit_pct: fit_pct(&rounds, 2.0),
                three_sf_fit_pct: fit_pct(&rounds, 1.0),
            });
        }
    }

    summaries
}

fn topology_names() -> [&'static str; 6] {
    [
        "evidence-lower-bound",
        "gossipsub-conservative",
        "gossipsub-optimized",
        "committee-hierarchical",
        "central-collector",
        "sequential-chain",
    ]
}

fn sample_params(rng: &mut StdRng) -> SampledParams {
    SampledParams {
        quorum: rng.gen_range(MIN_QUORUM..=0.90),
        bandwidth_mbps: rng.gen_range(50.0..=500.0),
        evidence_bytes_per_validator: rng.gen_range(1.0..=16.0),
        raw_vote_bytes: rng.gen_range(96.0..=160.0),
        hop_ms: rng.gen_range(150.0..=900.0),
        duplicate_factor: rng.gen_range(1.0..=2.5),
        crypto_multiplier: rng.gen_range(0.75..=1.75),
        cores: rng.gen_range(4.0..=16.0),
    }
}

fn evaluate_trial(validators: usize, params: &SampledParams) -> Vec<TrialResult> {
    let evidence_floor = evidence_floor_ms(validators, params);
    let conservative = gossipsub_metrics(validators, params, 6.0, 10.0, 3.0, 5.0);
    let optimized = gossipsub_metrics(validators, params, 12.0, 20.0, 6.0, 12.0);

    vec![
        TrialResult {
            topology: "evidence-lower-bound",
            avg_degree: None,
            diameter_hops: None,
            broadcast_ms: evidence_floor,
            signature_aggregation_ms: 0.0,
            round_ms: evidence_floor,
        },
        TrialResult {
            topology: "gossipsub-conservative",
            ..conservative
        },
        TrialResult {
            topology: "gossipsub-optimized",
            ..optimized
        },
        committee_hierarchical_metrics(validators, params),
        central_collector_metrics(validators, params),
        sequential_chain_metrics(validators, params),
    ]
}

fn evidence_floor_ms(validators: usize, params: &SampledParams) -> f64 {
    let quorum_validators = validators as f64 * params.quorum;
    bits_to_ms(
        quorum_validators * params.evidence_bytes_per_validator * 8.0,
        params.bandwidth_mbps,
    )
}

fn gossipsub_metrics(
    validators: usize,
    params: &SampledParams,
    degree_min: f64,
    degree_max: f64,
    parallel_min: f64,
    parallel_max: f64,
) -> TrialResult {
    let mut rng = StdRng::seed_from_u64(
        validators as u64
            ^ params.bandwidth_mbps.to_bits()
            ^ params.hop_ms.to_bits()
            ^ degree_min.to_bits(),
    );
    let max_degree = validators.saturating_sub(1).max(1) as f64;
    let degree = rng.gen_range(degree_min..=degree_max).min(max_degree);
    let parallelism = rng.gen_range(parallel_min..=parallel_max).min(max_degree);
    let fanout = degree.min(parallelism).max(2.0);
    let quorum_validators = (validators as f64 * params.quorum).max(2.0);
    let dissemination_hops = quorum_validators.log(fanout).ceil().max(1.0);

    let evidence_floor = evidence_floor_ms(validators, params);
    let broadcast = dissemination_hops * params.hop_ms * params.duplicate_factor;
    let distributed_crypto = aggregate_crypto_ms(quorum_validators, params) / fanout;
    let round_ms = evidence_floor.max(broadcast).max(distributed_crypto);

    TrialResult {
        topology: "",
        avg_degree: Some(degree),
        diameter_hops: Some(random_mesh_diameter(validators, degree)),
        broadcast_ms: broadcast,
        signature_aggregation_ms: distributed_crypto,
        round_ms,
    }
}

fn committee_hierarchical_metrics(validators: usize, params: &SampledParams) -> TrialResult {
    let mut rng = StdRng::seed_from_u64(
        validators as u64 ^ params.raw_vote_bytes.to_bits() ^ params.crypto_multiplier.to_bits(),
    );
    let committee_size: f64 = rng.gen_range(256.0_f64..=2048.0_f64).min(validators as f64);
    let fanout: f64 = rng.gen_range(16.0_f64..=64.0_f64);
    let quorum_validators = validators as f64 * params.quorum;
    let committees = (quorum_validators / committee_size).ceil().max(1.0);
    let levels = if committees <= 1.0 {
        1.0
    } else {
        committees
            .log(fanout.min(committees).max(2.0))
            .ceil()
            .max(1.0)
    };

    let evidence_floor = evidence_floor_ms(validators, params);
    let local_receive = bits_to_ms(
        committee_size * params.raw_vote_bytes * 8.0,
        params.bandwidth_mbps,
    );
    let local_crypto = aggregate_crypto_ms(committee_size, params) / params.cores;
    let signature_aggregation_ms = local_receive + local_crypto;
    let broadcast_ms = levels * params.hop_ms * params.duplicate_factor;
    let graph_nodes = validators as f64 + committees;
    let avg_degree = tree_average_degree(graph_nodes);
    let diameter_hops = if committees <= 1.0 {
        2.0
    } else {
        2.0 + 2.0 * levels
    };
    let round_ms = evidence_floor
        .max(signature_aggregation_ms)
        .max(broadcast_ms);

    TrialResult {
        topology: "committee-hierarchical",
        avg_degree: Some(avg_degree),
        diameter_hops: Some(diameter_hops),
        broadcast_ms,
        signature_aggregation_ms,
        round_ms,
    }
}

fn central_collector_metrics(validators: usize, params: &SampledParams) -> TrialResult {
    let quorum_validators = validators as f64 * params.quorum;
    let evidence_floor = evidence_floor_ms(validators, params);
    let raw_receive = bits_to_ms(
        quorum_validators * params.raw_vote_bytes * 8.0,
        params.bandwidth_mbps,
    );
    let crypto = aggregate_crypto_ms(quorum_validators, params) / params.cores;
    let signature_aggregation_ms = raw_receive + crypto;
    let broadcast_ms = params.hop_ms;
    let round_ms = evidence_floor.max(signature_aggregation_ms + broadcast_ms);

    TrialResult {
        topology: "central-collector",
        avg_degree: Some(tree_average_degree(validators as f64)),
        diameter_hops: Some(if validators <= 2 { 1.0 } else { 2.0 }),
        broadcast_ms,
        signature_aggregation_ms,
        round_ms,
    }
}

fn sequential_chain_metrics(validators: usize, params: &SampledParams) -> TrialResult {
    let quorum_validators = validators as f64 * params.quorum;
    let evidence_floor = evidence_floor_ms(validators, params);
    let broadcast_ms = quorum_validators * params.hop_ms;
    let signature_aggregation_ms = aggregate_crypto_ms(quorum_validators, params);
    let round_ms = evidence_floor
        .max(broadcast_ms)
        .max(signature_aggregation_ms);

    TrialResult {
        topology: "sequential-chain",
        avg_degree: Some(tree_average_degree(validators as f64)),
        diameter_hops: Some(validators.saturating_sub(1) as f64),
        broadcast_ms,
        signature_aggregation_ms,
        round_ms,
    }
}

fn random_mesh_diameter(validators: usize, degree: f64) -> f64 {
    if validators <= 1 {
        return 0.0;
    }
    if degree >= validators.saturating_sub(1) as f64 {
        return 1.0;
    }

    let branching = (degree - 1.0).max(2.0);
    (validators as f64).log(branching).ceil().max(1.0)
}

fn tree_average_degree(nodes: f64) -> f64 {
    if nodes <= 1.0 {
        0.0
    } else {
        2.0 * (nodes - 1.0) / nodes
    }
}

fn aggregate_crypto_ms(items: f64, params: &SampledParams) -> f64 {
    items * (VERIFY_MS + AGG_SIG_MS + AGG_PUB_MS) * params.crypto_multiplier
}

fn bits_to_ms(bits: f64, bandwidth_mbps: f64) -> f64 {
    bits / (bandwidth_mbps * 1_000_000.0) * 1000.0
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }

    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn optional_percentile(sorted: &[f64], p: f64) -> Option<f64> {
    (!sorted.is_empty()).then(|| percentile(sorted, p))
}

fn sort_f64(values: &mut [f64]) {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
}

fn fit_pct(sorted: &[f64], protocol_rounds: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }

    let fits = sorted
        .iter()
        .filter(|round_ms| **round_ms * protocol_rounds <= SLOT_MS)
        .count();

    fits as f64 / sorted.len() as f64 * 100.0
}

fn print_csv(summaries: &[Summary]) {
    println!("validators,topology,p50_avg_degree,p50_diameter_hops,p50_broadcast_ms,p50_signature_aggregation_ms,p50_round_ms,p90_round_ms,p99_round_ms,ssf_p50_slot_ms,three_sf_p50_slot_ms,three_sf_p50_finality_ms,ssf_fit_12s_pct,three_sf_fit_12s_pct");
    for row in summaries {
        println!(
            "{},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.1},{:.1}",
            row.validators,
            row.topology,
            format_optional(row.p50_avg_degree),
            format_optional(row.p50_diameter_hops),
            row.p50_broadcast_ms,
            row.p50_signature_aggregation_ms,
            row.p50_round_ms,
            row.p90_round_ms,
            row.p99_round_ms,
            row.p50_round_ms * 2.0,
            row.p50_round_ms,
            row.p50_round_ms * 3.0,
            row.ssf_fit_pct,
            row.three_sf_fit_pct
        );
    }
}

fn format_optional(value: Option<f64>) -> String {
    value.map_or_else(String::new, |value| format!("{value:.3}"))
}

fn print_table(summaries: &[Summary], opts: &Options) {
    println!(
        "Monte Carlo standalone SSF/3SF aggregation timing: trials={} seed={}",
        opts.trials, opts.seed
    );
    println!(
        "Sampled ranges: quorum=[0.667,0.90], bandwidth=[50,500] Mbps, evidence_bytes_per_validator=[1,16], raw_vote_bytes=[96,160], hop_ms=[150,900], duplicate_factor=[1.0,2.5], cores=[4,16]"
    );
    println!(
        "{:<11} {:<24} {:>7} {:>7} {:>10} {:>10} {:>10} {:>9} {:>9}",
        "validators",
        "topology",
        "degree",
        "diam",
        "broadcast",
        "sig_agg",
        "round",
        "SSF_fit%",
        "3SF_fit%"
    );
    for row in summaries {
        println!(
            "{:<11} {:<24} {:>7} {:>7} {:>10.0} {:>10.0} {:>10.0} {:>9.1} {:>9.1}",
            row.validators,
            row.topology,
            format_optional(row.p50_avg_degree),
            format_optional(row.p50_diameter_hops),
            row.p50_broadcast_ms,
            row.p50_signature_aggregation_ms,
            row.p50_round_ms,
            row.ssf_fit_pct,
            row.three_sf_fit_pct
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_mesh_diameter_grows_slowly() {
        assert!(random_mesh_diameter(1_000_000, 8.0) < 10.0);
        assert_eq!(random_mesh_diameter(10, 9.0), 1.0);
    }

    #[test]
    fn every_round_respects_its_components_and_evidence_floor() {
        let params = SampledParams {
            quorum: 2.0 / 3.0,
            bandwidth_mbps: 100.0,
            evidence_bytes_per_validator: 8.0,
            raw_vote_bytes: 128.0,
            hop_ms: 300.0,
            duplicate_factor: 1.5,
            crypto_multiplier: 1.0,
            cores: 8.0,
        };
        let evidence_floor = evidence_floor_ms(1_000_000, &params);

        for result in evaluate_trial(1_000_000, &params) {
            assert!(result.round_ms >= evidence_floor);
            assert!(result.round_ms >= result.broadcast_ms);
            assert!(
                result.round_ms >= result.signature_aggregation_ms
                    || result.topology == "central-collector"
            );
        }
    }
}
