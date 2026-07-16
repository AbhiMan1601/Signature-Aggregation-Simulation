use std::env;

const VERIFY_MS: f64 = 882.21440825 / 1_000_000.0 * 1000.0;
const AGG_SIG_MS: f64 = 1.068981688 / 1_000_000.0 * 1000.0;
const AGG_PUB_MS: f64 = 0.460724574 / 1_000_000.0 * 1000.0;

const SLOT_MS: f64 = 12_000.0;
const SLOTS_PER_EPOCH: f64 = 32.0;
const MIN_QUORUM: f64 = 2.0 / 3.0;
const TARGET_COMMITTEE_SIZE: f64 = 128.0;
const MAX_COMMITTEES_PER_SLOT: f64 = 64.0;
const MAX_VALIDATORS_PER_COMMITTEE: f64 = 2_048.0;
const TARGET_AGGREGATORS_PER_COMMITTEE: f64 = 16.0;
const GOSSIPSUB_D: f64 = 8.0;
const GOSSIPSUB_HEARTBEAT_MS: f64 = 700.0;
const MAX_GOSSIP_CLOCK_DISPARITY_MS: f64 = 500.0;
const ATTESTATION_BROADCAST_MS: f64 = SLOT_MS / 3.0;
const AGGREGATE_BROADCAST_MS: f64 = SLOT_MS * 2.0 / 3.0;
const RAW_ATTESTATION_BYTES: f64 = 128.0;
const AGGREGATE_BASE_BYTES: f64 = 256.0;
const DUPLICATE_FACTOR: f64 = 1.35;

#[derive(Debug, Clone)]
struct Options {
    validators: Vec<usize>,
    csv: bool,
}

#[derive(Debug, Clone, Copy)]
struct NodeProfile {
    name: &'static str,
    bandwidth_mbps: f64,
    cores: f64,
    source: &'static str,
}

#[derive(Debug, Clone)]
struct ResultRow {
    validators: usize,
    profile: NodeProfile,
    scenario: &'static str,
    committees_per_slot: f64,
    committee_size: f64,
    aggregate_messages: f64,
    local_aggregation_ms: f64,
    global_collection_ms: f64,
    round_ms: f64,
    ssf_slot_ms: f64,
    three_sf_slot_ms: f64,
    three_sf_finality_ms: f64,
    local_fits_aggregate_window: bool,
    ssf_fits_12s: bool,
    three_sf_fits_12s: bool,
}

fn main() {
    let opts = parse_args();
    let rows = run_profiles(&opts.validators);

    if opts.csv {
        print_csv(&rows);
    } else {
        print_table(&rows);
    }
}

fn parse_args() -> Options {
    let mut opts = Options {
        validators: vec![128_000, 256_000, 512_000, 1_000_000, 2_097_152, 2_500_000],
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
    println!("Usage: ethereum_node_latency [--csv] [--validators 128000,512000,1000000]");
    println!("Runs deterministic SSF/3SF aggregation timing against Ethereum node-level bandwidth and gossip timing profiles.");
}

fn node_profiles() -> [NodeProfile; 5] {
    [
        NodeProfile {
            name: "ethereum-org-minimum",
            bandwidth_mbps: 10.0,
            cores: 2.0,
            source: "ethereum.org minimum node requirement",
        },
        NodeProfile {
            name: "ethereum-org-recommended",
            bandwidth_mbps: 25.0,
            cores: 4.0,
            source: "ethereum.org recommended node requirement",
        },
        NodeProfile {
            name: "measured-raspberry-pi",
            bandwidth_mbps: 100.0,
            cores: 4.0,
            source: "Cortes-Goicoechea et al. Raspberry Pi profile",
        },
        NodeProfile {
            name: "measured-default-node",
            bandwidth_mbps: 250.0,
            cores: 4.0,
            source: "Cortes-Goicoechea et al. default node profile",
        },
        NodeProfile {
            name: "measured-fat-node",
            bandwidth_mbps: 10_000.0,
            cores: 32.0,
            source: "Cortes-Goicoechea et al. fat node profile",
        },
    ]
}

fn run_profiles(validators: &[usize]) -> Vec<ResultRow> {
    let mut rows = Vec::new();

    for &validators in validators {
        for profile in node_profiles() {
            rows.push(evaluate_committee_profile(validators, profile));
            rows.push(evaluate_global_quorum_profile(validators, profile));
        }
    }

    rows
}

fn evaluate_committee_profile(validators: usize, profile: NodeProfile) -> ResultRow {
    let validators_per_slot = validators as f64 / SLOTS_PER_EPOCH;
    let committees_per_slot = (validators_per_slot / TARGET_COMMITTEE_SIZE)
        .ceil()
        .clamp(1.0, MAX_COMMITTEES_PER_SLOT);
    let committee_size = (validators_per_slot / committees_per_slot)
        .ceil()
        .clamp(1.0, MAX_VALIDATORS_PER_COMMITTEE);
    let quorum_committee = committee_size * MIN_QUORUM;
    let aggregate_messages = committees_per_slot * TARGET_AGGREGATORS_PER_COMMITTEE;

    let local_receive_ms = bits_to_ms(
        quorum_committee * RAW_ATTESTATION_BYTES * 8.0 * DUPLICATE_FACTOR,
        profile.bandwidth_mbps,
    );
    let local_crypto_ms = aggregate_crypto_ms(quorum_committee) / profile.cores;
    let local_hops = quorum_committee.max(2.0).log(GOSSIPSUB_D).ceil();
    let local_gossip_ms = local_hops * (GOSSIPSUB_HEARTBEAT_MS + MAX_GOSSIP_CLOCK_DISPARITY_MS);
    let local_aggregation_ms = local_receive_ms.max(local_crypto_ms).max(local_gossip_ms);

    let aggregate_bytes = AGGREGATE_BASE_BYTES + (committee_size / 8.0).ceil();
    let global_receive_ms = bits_to_ms(
        aggregate_messages * aggregate_bytes * 8.0 * DUPLICATE_FACTOR,
        profile.bandwidth_mbps,
    );
    let global_crypto_ms = aggregate_crypto_ms(aggregate_messages) / profile.cores;
    let global_hops = aggregate_messages.max(2.0).log(GOSSIPSUB_D).ceil();
    let global_gossip_ms = global_hops * (GOSSIPSUB_HEARTBEAT_MS + MAX_GOSSIP_CLOCK_DISPARITY_MS);
    let global_collection_ms = global_receive_ms
        .max(global_crypto_ms)
        .max(global_gossip_ms);

    let round_ms = local_aggregation_ms + global_collection_ms;
    let ssf_slot_ms = 2.0 * round_ms;
    let three_sf_slot_ms = round_ms;
    let three_sf_finality_ms = 3.0 * round_ms;

    ResultRow {
        validators,
        profile,
        scenario: "ethereum-committee-subnets",
        committees_per_slot,
        committee_size,
        aggregate_messages,
        local_aggregation_ms,
        global_collection_ms,
        round_ms,
        ssf_slot_ms,
        three_sf_slot_ms,
        three_sf_finality_ms,
        local_fits_aggregate_window: local_aggregation_ms
            <= AGGREGATE_BROADCAST_MS - ATTESTATION_BROADCAST_MS,
        ssf_fits_12s: ssf_slot_ms <= SLOT_MS,
        three_sf_fits_12s: three_sf_slot_ms <= SLOT_MS,
    }
}

fn evaluate_global_quorum_profile(validators: usize, profile: NodeProfile) -> ResultRow {
    let quorum_validators = validators as f64 * MIN_QUORUM;
    let raw_receive_ms = bits_to_ms(
        quorum_validators * RAW_ATTESTATION_BYTES * 8.0 * DUPLICATE_FACTOR,
        profile.bandwidth_mbps,
    );
    let crypto_ms = aggregate_crypto_ms(quorum_validators) / profile.cores;
    let gossip_hops = quorum_validators.max(2.0).log(GOSSIPSUB_D).ceil();
    let gossip_ms = gossip_hops * (GOSSIPSUB_HEARTBEAT_MS + MAX_GOSSIP_CLOCK_DISPARITY_MS);
    let round_ms = raw_receive_ms.max(crypto_ms).max(gossip_ms);
    let ssf_slot_ms = 2.0 * round_ms;
    let three_sf_slot_ms = round_ms;
    let three_sf_finality_ms = 3.0 * round_ms;

    ResultRow {
        validators,
        profile,
        scenario: "global-quorum-collector",
        committees_per_slot: 1.0,
        committee_size: quorum_validators,
        aggregate_messages: quorum_validators,
        local_aggregation_ms: round_ms,
        global_collection_ms: 0.0,
        round_ms,
        ssf_slot_ms,
        three_sf_slot_ms,
        three_sf_finality_ms,
        local_fits_aggregate_window: round_ms <= AGGREGATE_BROADCAST_MS - ATTESTATION_BROADCAST_MS,
        ssf_fits_12s: ssf_slot_ms <= SLOT_MS,
        three_sf_fits_12s: three_sf_slot_ms <= SLOT_MS,
    }
}

fn aggregate_crypto_ms(items: f64) -> f64 {
    items * (VERIFY_MS + AGG_SIG_MS + AGG_PUB_MS)
}

fn bits_to_ms(bits: f64, bandwidth_mbps: f64) -> f64 {
    bits / (bandwidth_mbps * 1_000_000.0) * 1000.0
}

fn print_csv(rows: &[ResultRow]) {
    println!("validators,scenario,profile,bandwidth_mbps,cores,committees_per_slot,committee_size,aggregate_messages,local_aggregation_ms,global_collection_ms,round_ms,ssf_slot_ms,three_sf_slot_ms,three_sf_finality_ms,local_fits_4s_window,ssf_fits_12s,three_sf_fits_12s,source");
    for row in rows {
        println!(
            "{},{},{},{:.0},{:.0},{:.0},{:.0},{:.0},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{},{},{}",
            row.validators,
            row.scenario,
            row.profile.name,
            row.profile.bandwidth_mbps,
            row.profile.cores,
            row.committees_per_slot,
            row.committee_size,
            row.aggregate_messages,
            row.local_aggregation_ms,
            row.global_collection_ms,
            row.round_ms,
            row.ssf_slot_ms,
            row.three_sf_slot_ms,
            row.three_sf_finality_ms,
            row.local_fits_aggregate_window,
            row.ssf_fits_12s,
            row.three_sf_fits_12s,
            row.profile.source
        );
    }
}

fn print_table(rows: &[ResultRow]) {
    println!("Ethereum node-level SSF/3SF signature aggregation timing");
    println!(
        "Protocol constants: slot=12s, attestation broadcast=4s, aggregate broadcast=8s, gossipsub D=8, heartbeat=0.7s, max clock disparity=0.5s"
    );
    println!(
        "{:<10} {:<25} {:<24} {:>8} {:>6} {:>9} {:>9} {:>10} {:>10} {:>9} {:>9} {:>9}",
        "validators",
        "scenario",
        "profile",
        "Mbps",
        "cores",
        "committee",
        "local_ms",
        "global_ms",
        "round_ms",
        "SSF_fit",
        "3SF_fit",
        "4s_fit"
    );
    for row in rows {
        println!(
            "{:<10} {:<25} {:<24} {:>8.0} {:>6.0} {:>9.0} {:>9.0} {:>10.0} {:>10.0} {:>9} {:>9} {:>9}",
            row.validators,
            row.scenario,
            row.profile.name,
            row.profile.bandwidth_mbps,
            row.profile.cores,
            row.committee_size,
            row.local_aggregation_ms,
            row.global_collection_ms,
            row.round_ms,
            bool_label(row.ssf_fits_12s),
            bool_label(row.three_sf_fits_12s),
            bool_label(row.local_fits_aggregate_window),
        );
    }
}

fn bool_label(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
