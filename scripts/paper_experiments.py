#!/usr/bin/env python3
"""Versioned configurations, execution, clustered analysis, and five paper panels.
Requires Python 3.12, matplotlib 3.11.2 and numpy 2.5.3 for plot/analysis.
All scientific observations come from Rust JSONL; no paper rates are used in networks.
"""
import argparse
import gzip
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target/release/paper_ctmc"
VERSION = "paper-suite-v1"
# This source file is also the versioned experiment configuration. Exported verbatim
# in the dataset; no separate configuration files are needed.
PLAN = {
    "version": VERSION,
    "ctmc_grid": [16, 64, 256, 1024, 4096, 16384, 65536, 262144, 1000000],
    "capacity_laws": ["fixed", "sqrt", "linear"],
    "ctmc_seeds": [17, 43, 91],
    "ctmc_trials_per_seed": 2000,
    "ctmc_sample_max_n": 4096,
    "network_grid": [32, 128, 512, 2048],
    "topologies": ["mesh", "central", "hierarchy"],
    "capacity_regimes": ["per_node", "global"],
    "topology_seeds": [11, 23, 37, 53, 71, 89, 107, 131],
    "delay_trials_per_topology": 12,
    "network_bandwidth_bytes_s": 12500000,
    "global_bandwidth_bytes_s": 12500000,
    "propagation_s": .01,
    "jitter_s": .005,
    "proposal_bytes": 4096,
    "degree": 8,
    "fanout": 4,
    "workers": 1,
    "bls_trials": 200,
    "bootstrap_resamples": 2000,
    "wall_budget_s": 600,
    "stopping_rule": "stop before next configuration after wall budget; retain completed observations",
    "ssf_source": "https://arxiv.org/pdf/2302.12745v1 (Algorithm 3, Section 7 acknowledgments)",
    "three_sf_source": "https://arxiv.org/html/2411.00558v1 (Algorithm 7, Section 7.1)",
}

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()

class Runner:
    def __init__(self):
        self.p = subprocess.Popen([str(BIN)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=None, text=True, bufsize=1)
    def run(self, config):
        self.p.stdin.write(json.dumps(config, allow_nan=False) + "\n")
        self.p.stdin.flush()
        line = self.p.stdout.readline()
        if not line:
            raise RuntimeError(f"Rust runner exited ({self.p.poll()}) for {config}")
        result = json.loads(line)
        result["config_sha256"] = hashlib.sha256(json.dumps(result["config"], sort_keys=True).encode()).hexdigest()
        return result
    def close(self):
        self.p.stdin.close()
        if self.p.wait() != 0:
            raise RuntimeError("Rust runner failed")

def capacity(n, law):
    return {"fixed": 8., "sqrt": math.sqrt(n), "linear": float(n)}[law]

def base_network(calibration):
    return {"mode": "network", "bandwidth_bytes_s": PLAN["network_bandwidth_bytes_s"],
            "propagation_s": PLAN["propagation_s"], "jitter_s": PLAN["jitter_s"],
            "proposal_bytes": PLAN["proposal_bytes"], "degree": PLAN["degree"],
            "fanout": PLAN["fanout"], "workers": PLAN["workers"],
            "sign_s": calibration["sign"]["mean_s"],
            "verify_s": calibration["verify"]["mean_s"],
            "pk_s": calibration["pk_per_add"]["mean_s"],
            "merge_s": calibration["merge_per_add"]["mean_s"]}

def suite(args):
    if args.output.exists():
        raise SystemExit(f"Refusing to replace {args.output}; choose --output for a new run.")
    began = time.monotonic()
    runner = Runner()
    bench = runner.run({"mode": "bls", "trials": PLAN["bls_trials"], "seed": 7})
    smoke = runner.run({**base_network(bench), "n": 32, "m": 32, "trials": 2})
    smoke_large = runner.run({**base_network(bench), "n": 2048, "m": 2048, "trials": 1})
    print(f"Smoke: n=32 {smoke['runtime_s']:.3f}s; n=2048 {smoke_large['runtime_s']:.3f}s", flush=True)
    data = {"schema": VERSION, "plan": PLAN, "calibration": bench, "smoke": [smoke, smoke_large],
            "ctmc": [], "network": [], "stopped": False}
    for law in PLAN["capacity_laws"]:
        for n in PLAN["ctmc_grid"]:
            seeds = PLAN["ctmc_seeds"] if n <= PLAN["ctmc_sample_max_n"] else [17]
            for seed in seeds:
                c = {"n": n, "ic": capacity(n, law), "seed": seed,
                     "trials": PLAN["ctmc_trials_per_seed"] if len(seeds) > 1 else 0,
                     "deadlines_s": [1., 12., 60.]}
                data["ctmc"].append({"law": law, "group": "scaling", **runner.run(c)})
    # One-factor sensitivity, fixed global quorum; Q=A uses exact harmonic form.
    for degree in [4., 8., 16.]:
        for rate in [.5, 1., 2.]:
            data["ctmc"].append({"group": "rate_degree", "law": "fixed", **runner.run(
                {"n": 1024, "degree": degree, "lambda": rate, "trials": 2000, "seed": 121})})
    for a in [1024, 922, 768, 683, 614, 1]:
        data["ctmc"].append({"group": "availability", "law": "fixed", **runner.run(
            {"n": 1024, "a": a, "trials": 2000, "seed": 179})})
    for c in [{"n": 1, "q": 1}, {"n": 128, "q": 128}, {"n": 128, "a": 1, "q": 1}]:
        data["ctmc"].append({"group": "boundary", "law": "fixed", **runner.run({**c, "trials": 2000, "seed": 197})})
    configs = []
    for n in PLAN["network_grid"]:
        for topology in PLAN["topologies"]:
            for regime in PLAN["capacity_regimes"]:
                for ts in PLAN["topology_seeds"]:
                    configs.append({**base_network(bench), "n": n, "m": n, "topology": topology,
                        "global_bytes_s": PLAN["global_bandwidth_bytes_s"] if regime == "global" else None,
                        "topology_seed": ts, "seed": 1000 + ts, "trials": PLAN["delay_trials_per_topology"]})
    # Fixed physical resources, correlated benign unavailability and all-signatures boundary.
    for n, m, a, q, tag in [(128, 32, 128, None, "multi_key"), (512, 32, 512, None, "multi_key"),
                           (2048, 32, 2048, None, "multi_key"), (128, 128, 96, None, "offline"),
                           (128, 128, 80, None, "unattainable"), (128, 128, 128, 128, "all_signers")]:
        for topology in PLAN["topologies"]:
            for ts in PLAN["topology_seeds"]:
                configs.append({**base_network(bench), "n": n, "m": m, "a": a, "q": q,
                    "topology": topology, "topology_seed": ts, "seed": 1000+ts,
                    "trials": PLAN["delay_trials_per_topology"], "tag": tag})
    for i, cfg in enumerate(configs):
        if time.monotonic() - began > PLAN["wall_budget_s"]:
            data["stopped"] = True
            data["pending_configs"] = configs[i:]
            break
        tag = cfg.pop("tag", "baseline")
        data["network"].append({"tag": tag, **runner.run(cfg)})
        if i % 48 == 0:
            print(f"Network {i+1}/{len(configs)}, elapsed {time.monotonic()-began:.1f}s", flush=True)
    initial = analyze(data)
    data["prediction_trials"] = []
    for p in initial["predictions"]:
        if p["n"] > p["calibration_n"]:
            data["prediction_trials"].append({"topology": p["topology"], "regime": p["regime"], **runner.run(
                {"n": p["n"], "ic": 8., "degree": 8., "lambda": p["lambda_degree"]/8.,
                 "trials": 2000, "seed": 307, "deadlines_s": [.1, .5, 1., 2., 5., 12.]})})
    runner.close()
    data["provenance"] = {
        "commit": command("git", "rev-parse", "HEAD"), "branch": command("git", "branch", "--show-current"),
        "manuscript_sha256": sha(args.manuscript), "manuscript_path": str(args.manuscript),
        "rustc": command("rustc", "--version"), "cargo": command("cargo", "--version"),
        "cpu": command("lscpu"), "platform": platform.platform(), "python": sys.version,
        "source_sha256": {str(p.relative_to(ROOT)): sha(p) for p in
                           [ROOT/"src/bin/paper_ctmc.rs", Path(__file__), ROOT/"Cargo.lock"]},
        "compile": "cargo build --offline --release --bin paper_ctmc (default release profile)",
        "elapsed_s": time.monotonic()-began, "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "cache_policy": "32-key warm BLS loop, one thread; generated trusted keys; not a deployment distribution",
    }
    data["analysis"] = analyze(data)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with gzip.open(args.output, "wt") as f:
        json.dump(data, f, allow_nan=False, separators=(",", ":"))
    print(json.dumps(data["analysis"]["findings"], indent=2))
    print(f"Saved {args.output}; elapsed {data['provenance']['elapsed_s']:.2f}s")

def analyze(data):
    import numpy as np
    rng = np.random.default_rng(937)
    grouped = {}
    for row in data["network"]:
        c = row["config"]
        key = (row["tag"], c["n"], c["m"], row["a"], row["q"], c["topology"],
               "global" if c["global_bytes_s"] else "per_node")
        grouped.setdefault(key, []).append(row)
    summaries = []
    fields = ["proposal_s", "aggregation_s", "proof_s", "ssf_required_slot_s", "three_sf_required_slot_s", "three_sf_finality_s"]
    for key, rows in grouped.items():
        out = dict(zip(["tag", "n", "m", "a", "q", "topology", "regime"], key))
        out["attainable"] = rows[0]["attainable"]
        out["clusters"] = len(rows)
        out["distinct_topology_draws"] = len(rows) if out["topology"] == "mesh" else 1
        out["trials_per_topology"] = rows[0]["config"]["trials"]
        if not out["attainable"]:
            summaries.append(out)
            continue
        for field in fields:
            clusters = np.array([[trial[field] for trial in r["raw"]] for r in rows])
            means = clusters.mean(axis=1)
            resamples = rng.integers(0, len(rows), (PLAN["bootstrap_resamples"], len(rows)))
            boot = means[resamples].mean(axis=1)
            # Cluster bootstrap resamples independent graph draws; fixed graph delay SE separate.
            out[field] = {"mean": float(clusters.mean()), "ci95": np.quantile(boot,[.025,.975]).tolist(),
                "within_topology_variance_mean": float(np.var(clusters,axis=1,ddof=1).mean()),
                "between_topology_mean_variance": float(np.var(means,ddof=1)),
                "quantiles": np.quantile(clusters,[.5,.95,.99]).tolist(),
                "quantile_ci95": [np.quantile(np.quantile(clusters[resamples].reshape(len(resamples),-1),p,axis=1),[.025,.975]).tolist() for p in [.5,.95,.99]]}
        out["metrics_mean"] = {k: statistics.mean(t["metrics"][k] for r in rows for t in r["raw"]) for k in rows[0]["raw"][0]["metrics"]}
        out["deadline_probabilities"] = {}
        for field in fields:
            clusters = np.array([[trial[field] for trial in r["raw"]] for r in rows])
            results = []
            for d in [.1,.5,1.,2.,5.,12.]:
                rates = (clusters<=d).mean(axis=1)
                ids = rng.integers(0,len(rows),(2000,len(rows)))
                # Independent Bernoulli trials under each fixed topology get Wilson CIs.
                per_cluster = []
                for xs in clusters:
                    k=int((xs<=d).sum()); n=len(xs); z=1.959963984540054
                    phat=k/n; z2=z*z/n; mid=(phat+z2/2)/(1+z2)
                    half=z*math.sqrt(phat*(1-phat)/n+z2/(4*n))/(1+z2)
                    per_cluster.append({"successes":k,"trials":n,"estimate":phat,"wilson_ci95":[max(0.,mid-half),min(1.,mid+half)]})
                # A single predeclared trial per independent draw supports an unconditional
                # binomial interval, including nonzero bounds when no failures are seen.
                k=int((clusters[:,0]<=d).sum()); n=len(rows); phat=k/n; z2=z*z/n
                mid=(phat+z2/2)/(1+z2); half=z*math.sqrt(phat*(1-phat)/n+z2/(4*n))/(1+z2)
                results.append({"deadline_s":d,"estimate":float(rates.mean()),
                    "cluster_bootstrap_ci95":np.quantile(rates[ids].mean(axis=1),[.025,.975]).tolist(),
                    "independent_draw_ci95":[max(0.,mid-half),min(1.,mid+half)],"fixed_topology":per_cluster})
            out["deadline_probabilities"][field]=results
        out["pipeline_success"] = [statistics.mean(t["pipeline"][i]["success"] for r in rows for t in r["raw"]) for i in [0,1]]
        out["cpu_utilization_mean"] = statistics.mean(t["cpu_utilization"] for r in rows for t in r["raw"])
        summaries.append(out)
    # Calibration uses only N=32 broadcast. Product lambda*D identifiable;
    # fixed ic=8 declared before fitting; aggregation receives the same fitted product.
    predictions = []
    for topology in PLAN["topologies"]:
        for regime in PLAN["capacity_regimes"]:
            selected = sorted([s for s in summaries if s["tag"] == "baseline" and s["topology"] == topology and s["regime"] == regime],key=lambda s:s["n"])
            if not selected:
                continue
            n0 = selected[0]["n"]
            def hb(n): return math.fsum(1/i for i in range(1,n))
            def bnum(n): return (n-1)*hb(n)*(2/n+1/8)
            ld = bnum(n0)/selected[0]["proposal_s"]["mean"]
            for s in selected:
                n,q=s["n"],s["q"]
                pred=bnum(n)/ld
                hq=hb(q)
                ha=hb(n)
                haq=math.fsum(1/i for i in range(1,n-q+1))
                pred_a=(n-1)/ld*((hq+ha-haq)/n+hq/8)
                baseline=selected[0]["proposal_s"]["mean"]*math.log(n)/math.log(n0)
                linear=selected[0]["aggregation_s"]["mean"]*(q-1)/(selected[0]["q"]-1)
                predictions.append({"topology":topology,"regime":regime,"n":n,"calibration_n":n0,"lambda_degree":ld,
                    "measured_b_s":s["proposal_s"]["mean"],"predicted_b_s":pred,"relative_b_error":pred/s["proposal_s"]["mean"]-1,
                    "measured_a_s":s["aggregation_s"]["mean"],"predicted_a_s":pred_a,"relative_a_error":pred_a/s["aggregation_s"]["mean"]-1,
                    "log_baseline_b_s":baseline,"linear_baseline_a_s":linear,
                    "ctmc_b_variance_s2":sum(((n-1)/ld*(1/i+1/8)/(n-i))**2 for i in range(1,n)),
                    "measured_b_variance_s2":s["proposal_s"]["within_topology_variance_mean"]})
    # MC consistency is a report, never a flaky pass/fail test.
    zs=[]
    for row in data["ctmc"]:
        for model,mc in [("broadcast","mc_broadcast"),("aggregation","mc_aggregation")]:
            if row[mc] and row[model] and row[mc]["se_mean_s"]:
                zs.append({"config_sha256":row["config_sha256"],"component":model,
                           "z":(row[mc]["mean_s"]-row[model]["mean_s"])/row[mc]["se_mean_s"]})
    findings={"ctmc_configs":len(data["ctmc"]),"network_configs":len(data["network"]),
              "network_trials":sum(len(x["raw"]) for x in data["network"]),
              "max_abs_mc_z":max(abs(x["z"]) for x in zs),
              "z_over_1_96_count":sum(abs(x["z"])>1.96 for x in zs),"mc_comparisons":len(zs),
              "heldout": [p for p in predictions if p["n"]==2048]}
    return {"network_summary":summaries,"predictions":predictions,"mc_checks":zs,"findings":findings}

def load(path):
    with gzip.open(path,"rt") as f: return json.load(f)

def plot(args):
    os.environ.setdefault("MPLCONFIGDIR", "/tmp/icbc-matplotlib")
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import numpy as np
    data=load(args.output)
    plt.rcParams.update({"font.size":10,"axes.spines.top":False,"axes.spines.right":False,"svg.fonttype":"none"})
    colors=["#0072B2","#D55E00","#009E73"]
    fig=plt.figure(figsize=(15,12),layout="constrained")
    gs=fig.add_gridspec(3,2)
    axes=[fig.add_subplot(gs[0,0]),fig.add_subplot(gs[0,1]),fig.add_subplot(gs[1,0]),fig.add_subplot(gs[1,1]),fig.add_subplot(gs[2,:])]
    ax=axes[0]
    for j,comp in enumerate(["broadcast","aggregation"]):
        rows=[r for r in data["ctmc"] if r["group"]=="scaling" and r["law"]=="fixed" and r["mc_"+comp]]
        ns=sorted(set(r["config"]["n"] for r in rows))
        means=[statistics.mean(r["mc_"+comp]["mean_s"] for r in rows if r["config"]["n"]==n) for n in ns]
        errors=[1.96*math.sqrt(sum(r[comp]["variance_s2"]*r["config"]["trials"] for r in rows if r["config"]["n"]==n))/sum(r["config"]["trials"] for r in rows if r["config"]["n"]==n) for n in ns]
        exact=[next(r[comp]["mean_s"] for r in rows if r["config"]["n"]==n) for n in ns]
        ax.plot(ns,exact,color=colors[j],label=f"Exact {comp}")
        ax.errorbar(ns,means,yerr=errors,fmt="o",color=colors[j],capsize=3,label=f"MC {comp}, 95% mean CI")
    ax.set(xscale="log",yscale="log",xlabel="Validator keys N",ylabel="Completion (s)",title="A. Mathematical implementation verification")
    ax.legend(fontsize=8)
    ax=axes[1]
    for j,law in enumerate(PLAN["capacity_laws"]):
        rows=[r for r in data["ctmc"] if r["group"]=="scaling" and r["law"]==law]
        ns=sorted(set(r["config"]["n"] for r in rows))
        ys=[next(r["broadcast"]["mean_s"] for r in rows if r["config"]["n"]==n)/(math.log(n)*{"fixed":n,"sqrt":math.sqrt(n),"linear":1}[law]) for n in ns]
        ax.plot(ns,ys,"o-",color=colors[j],label={"fixed":"ic=8: T/(N log N)","sqrt":"ic=√N: T/(√N log N)","linear":"ic=N: T/log N"}[law])
    ax.set(xscale="log",yscale="log",xlabel="Validator keys N",ylabel="Normalized broadcast mean (s)",title="B. Capacity-dependent scaling, exact moments")
    ax.legend(fontsize=8)
    ax=axes[2]
    ps=data["analysis"]["predictions"]
    for j,top in enumerate(PLAN["topologies"]):
        for regime,style in [("per_node","-"),("global","--")]:
            rows=[p for p in ps if p["topology"]==top and p["regime"]==regime]
            ax.plot([p["n"] for p in rows],[p["relative_b_error"] for p in rows],style+"o",color=colors[j],label=f"{top}, {regime}")
    ax.axhline(0,color="0.4",lw=1)
    ax.set(xscale="log",xlabel="Network nodes M=N (32 used for calibration)",ylabel="Broadcast (prediction / observation) − 1",title="C. Held-out fixed-ic CTMC prediction errors")
    ax.legend(fontsize=8,ncol=2)
    ax=axes[3]
    for j,top in enumerate(PLAN["topologies"]):
        rows=sorted([s for s in data["analysis"]["network_summary"] if s["tag"]=="baseline" and s["topology"]==top and s["regime"]=="per_node"],key=lambda s:s["n"])
        for field,style,label in [("aggregation_s","-","collection"),("ssf_required_slot_s","--","SSF path")]:
            ys=[s[field]["mean"] for s in rows]
            ci=np.array([s[field]["ci95"] for s in rows])
            ax.errorbar([s["n"] for s in rows],ys,yerr=np.array([ys-ci[:,0],ci[:,1]-ys]),fmt=style+"o",color=colors[j],capsize=2,label=f"{top} {label}")
    ax.set(xscale="log",yscale="log",xlabel="Validator keys = network nodes",ylabel="Latency (s)",title="D. Queued architectures, complete declared costs")
    ax.legend(fontsize=8,ncol=2)
    ax=axes[4]
    for j,top in enumerate(PLAN["topologies"]):
        rows=[r for r in data["network"] if r["tag"]=="baseline" and r["config"]["n"]==2048 and r["config"]["topology"]==top and r["config"]["global_bytes_s"] is None]
        for field,style,label in [("ssf_required_slot_s","-","SSF sequential acknowledgment path"),("three_sf_required_slot_s","--","3SF 5Δ service envelope")]:
            if not rows: continue
            clusters=np.array([[t[field] for t in r["raw"]] for r in rows])
            xs=np.linspace(0,float(clusters.max())*1.1,160)
            p=(clusters[:,:,None]<=xs).mean(axis=1)
            ids=np.random.default_rng(433).integers(0,len(rows),(2000,len(rows)))
            bs=p[ids].mean(axis=1)
            lo,hi=np.quantile(bs,[.025,.975],axis=0)
            k=(clusters[:,0,None]<=xs).sum(axis=0); n=len(rows); phat=k/n; z=1.959963984540054; z2=z*z/n
            mid=(phat+z2/2)/(1+z2); half=z*np.sqrt(phat*(1-phat)/n+z2/(4*n))/(1+z2)
            lo=np.minimum(lo,mid-half);hi=np.maximum(hi,mid+half)
            ax.plot(xs,p.mean(axis=0),style,color=colors[j],label=f"{top}: {label}")
            ax.fill_between(xs,lo,hi,color=colors[j],alpha=.08)
    ax.set(xlabel="Configured slot duration (s)",ylabel="Estimated service-envelope success",ylim=(-.02,1.02),title="E. Schedule timing at N=M=2048; honest timing abstraction")
    ax.legend(fontsize=8,ncol=2)
    for ax in axes: ax.grid(alpha=.18)
    fig.suptitle("Exact CTMC verification and independent eager-forwarding network experiments",fontsize=15)
    stem=ROOT/"reports/figures/paper-experiments"
    stem.parent.mkdir(parents=True,exist_ok=True)
    for ext in ["pdf","svg","png"]:fig.savefig(stem.with_suffix("."+ext),dpi=160)
    print(f"Saved {stem}.pdf/.svg/.png")

def single(args):
    config=json.loads(args.config)
    runner=Runner()
    try: print(json.dumps(runner.run(config),indent=2))
    finally: runner.close()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["run","plot","single","analyze"])
    parser.add_argument("--output",type=Path,default=ROOT/"reports/paper-results.json.gz")
    parser.add_argument("--manuscript",type=Path,default=Path("/home/abhimanyu/Downloads/finality_project_for_icbc.pdf"))
    parser.add_argument("--config",default='{"n":128,"trials":1000}')
    args=parser.parse_args()
    if args.action=="run":suite(args)
    elif args.action=="plot":plot(args)
    elif args.action=="single":single(args)
    else: print(json.dumps(analyze(load(args.output))["findings"],indent=2))
if __name__=="__main__":main()
