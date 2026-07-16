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
    round_ms: f64,
}

#[derive(Debug, Clone)]
struct Summary {
    validators: usize,
    topology: &'static str,
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
        validators: vec![128_000, 256_000, 512_000, 1_000_000, 2_097_152, 2_500_000],
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
    println!("Usage: compare_topologies [--csv] [--validators 128000,512000,1000000] [--trials 2000] [--seed 42]");
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
                    values.push(result.round_ms);
                }
            }
        }

        for (topology, mut values) in per_topology {
            values.sort_by(|a, b| a.partial_cmp(b).unwrap());
            summaries.push(Summary {
                validators,
                topology,
                p50_round_ms: percentile(&values, 0.50),
                p90_round_ms: percentile(&values, 0.90),
                p99_round_ms: percentile(&values, 0.99),
                ssf_fit_pct: fit_pct(&values, 2.0),
                three_sf_fit_pct: fit_pct(&values, 1.0),
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

    vec![
        TrialResult {
            topology: "evidence-lower-bound",
            round_ms: evidence_floor,
        },
        TrialResult {
            topology: "gossipsub-conservative",
            round_ms: gossipsub_round_ms(validators, params, 6.0, 10.0, 3.0, 5.0),
        },
        TrialResult {
            topology: "gossipsub-optimized",
            round_ms: gossipsub_round_ms(validators, params, 12.0, 20.0, 6.0, 12.0),
        },
        TrialResult {
            topology: "committee-hierarchical",
            round_ms: committee_hierarchical_round_ms(validators, params),
        },
        TrialResult {
            topology: "central-collector",
            round_ms: central_collector_round_ms(validators, params),
        },
        TrialResult {
            topology: "sequential-chain",
            round_ms: sequential_chain_round_ms(validators, params),
        },
    ]
}

fn evidence_floor_ms(validators: usize, params: &SampledParams) -> f64 {
    let quorum_validators = validators as f64 * params.quorum;
    bits_to_ms(
        quorum_validators * params.evidence_bytes_per_validator * 8.0,
        params.bandwidth_mbps,
    )
}

fn gossipsub_round_ms(
    validators: usize,
    params: &SampledParams,
    degree_min: f64,
    degree_max: f64,
    parallel_min: f64,
    parallel_max: f64,
) -> f64 {
    let mut rng = StdRng::seed_from_u64(
        validators as u64
            ^ params.bandwidth_mbps.to_bits()
            ^ params.hop_ms.to_bits()
            ^ degree_min.to_bits(),
    );
    let degree = rng.gen_range(degree_min..=degree_max);
    let parallelism = rng.gen_range(parallel_min..=parallel_max);
    let fanout = degree.min(parallelism).max(2.0);
    let quorum_validators = (validators as f64 * params.quorum).max(2.0);
    let hops = quorum_validators.log(fanout).ceil();

    let evidence_floor = evidence_floor_ms(validators, params);
    let propagation = hops * params.hop_ms * params.duplicate_factor;
    let distributed_crypto = aggregate_crypto_ms(quorum_validators, params) / fanout;

    evidence_floor.max(propagation).max(distributed_crypto)
}

fn committee_hierarchical_round_ms(validators: usize, params: &SampledParams) -> f64 {
    let mut rng = StdRng::seed_from_u64(
        validators as u64 ^ params.raw_vote_bytes.to_bits() ^ params.crypto_multiplier.to_bits(),
    );
    let committee_size: f64 = rng.gen_range(256.0..=2048.0);
    let fanout: f64 = rng.gen_range(16.0..=64.0);
    let quorum_validators = validators as f64 * params.quorum;
    let committees = (quorum_validators / committee_size).ceil().max(1.0);
    let levels = committees.log(fanout).ceil().max(1.0);

    let evidence_floor = evidence_floor_ms(validators, params);
    let local_receive = bits_to_ms(
        committee_size * params.raw_vote_bytes * 8.0,
        params.bandwidth_mbps,
    );
    let local_crypto = aggregate_crypto_ms(committee_size, params) / params.cores;
    let propagation = levels * params.hop_ms * params.duplicate_factor;

    evidence_floor
        .max(local_receive + local_crypto)
        .max(propagation)
}

fn central_collector_round_ms(validators: usize, params: &SampledParams) -> f64 {
    let quorum_validators = validators as f64 * params.quorum;
    let evidence_floor = evidence_floor_ms(validators, params);
    let raw_receive = bits_to_ms(
        quorum_validators * params.raw_vote_bytes * 8.0,
        params.bandwidth_mbps,
    );
    let crypto = aggregate_crypto_ms(quorum_validators, params) / params.cores;

    evidence_floor.max(raw_receive + crypto + params.hop_ms)
}

fn sequential_chain_round_ms(validators: usize, params: &SampledParams) -> f64 {
    let quorum_validators = validators as f64 * params.quorum;
    let evidence_floor = evidence_floor_ms(validators, params);
    evidence_floor.max(quorum_validators * params.hop_ms)
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
    println!("validators,topology,p50_round_ms,p90_round_ms,p99_round_ms,ssf_p50_slot_ms,three_sf_p50_slot_ms,three_sf_p50_finality_ms,ssf_fit_12s_pct,three_sf_fit_12s_pct");
    for row in summaries {
        println!(
            "{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.1},{:.1}",
            row.validators,
            row.topology,
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

fn print_table(summaries: &[Summary], opts: &Options) {
    println!(
        "Monte Carlo standalone SSF/3SF aggregation timing: trials={} seed={}",
        opts.trials, opts.seed
    );
    println!(
        "Sampled ranges: quorum=[0.667,0.90], bandwidth=[50,500] Mbps, evidence_bytes_per_validator=[1,16], raw_vote_bytes=[96,160], hop_ms=[150,900], duplicate_factor=[1.0,2.5], cores=[4,16]"
    );
    println!(
        "{:<11} {:<24} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "validators",
        "topology",
        "p50_round",
        "p90_round",
        "p99_round",
        "SSF_fit%",
        "3SF_fit%",
        "3SF_p50"
    );
    for row in summaries {
        println!(
            "{:<11} {:<24} {:>10.0} {:>10.0} {:>10.0} {:>10.1} {:>10.1} {:>10.0}",
            row.validators,
            row.topology,
            row.p50_round_ms,
            row.p90_round_ms,
            row.p99_round_ms,
            row.ssf_fit_pct,
            row.three_sf_fit_pct,
            row.p50_round_ms * 3.0
        );
    }
}
