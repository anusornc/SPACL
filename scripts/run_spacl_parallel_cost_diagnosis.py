#!/usr/bin/env python3
"""Pinned administrative-overhead diagnostic; not a parallel-benefit claim."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess


def run():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--warm", action="store_true")
    parser.add_argument("--assert-no-learning", action="store_true")
    args = parser.parse_args()
    if subprocess.run(["git","diff","--quiet","HEAD"]).returncode:
        raise RuntimeError("commit tracked source changes before capturing cost evidence")
    sources = ["src/reasoner/speculative.rs", "examples/prototype_spacl_parallel_cost.rs", __file__]
    subprocess.run(["git","ls-files","--error-unmatch",*sources],check=True,capture_output=True)
    args.output.mkdir(parents=True, exist_ok=False)
    cpus = sorted(os.sched_getaffinity(0))[:2]
    if len(cpus) < 2:
        raise RuntimeError("two allowed CPUs are required")
    manifest = {"schema_version":1, "label":args.label, "repeats":7, "cpus":cpus,
                "kind":"administrative_overhead_only", "promotable":False,
                "binary_sha256":hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                "probe_sha256":hashlib.sha256(Path("examples/prototype_spacl_parallel_cost.rs").read_bytes()).hexdigest(),
                "runtime_sha256":hashlib.sha256(Path("src/reasoner/speculative.rs").read_bytes()).hexdigest(),
                "runner_sha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                "cargo_lock_sha256":hashlib.sha256(Path("Cargo.lock").read_bytes()).hexdigest(),
                "tracked_source_clean":True,
                "build_profile":"release",
                "source_commit":subprocess.check_output(["git","rev-parse","HEAD"],text=True).strip()}
    (args.output/"manifest.json").write_text(json.dumps(manifest,sort_keys=True,indent=2)+"\n")
    rows=[]
    for case in ("sat","unsat"):
        for repeat in range(7):
            modes=("sequential","parallel") if repeat%2==0 else ("parallel","sequential")
            for mode in modes:
                for learning in (False,True):
                    cmd=["taskset","-c",",".join(map(str,cpus)),str(args.binary),
                         "--case",case,"--mode",mode,"--learning",str(int(learning)),
                         "--samples","1","--workers","2"]
                    if args.warm:
                        cmd += ["--warm"]
                    p=subprocess.run(cmd,capture_output=True,text=True,timeout=30)
                    if p.returncode:
                        raise RuntimeError(p.stderr)
                    response=json.loads(p.stdout)
                    if response["status"]!="complete":
                        raise RuntimeError("warm lifecycle barrier unavailable")
                    if args.warm and not response["warm_worker_completion_verified"]:
                        raise RuntimeError("warm completion proof missing")
                    row=response["rows"][0]
                    row.update(repeat=repeat,warm_worker_completion_verified=response["warm_worker_completion_verified"])
                    rows.append(row)
    (args.output/"rows.json").write_text(json.dumps(rows,sort_keys=True,indent=2)+"\n")
    failures=[r for r in rows if not r["matches_oracle"]]
    learning_violations=[r for r in rows if not r["learning"] and
                         (r["nogoods_learned"] or r["nogood_database_size"])]
    groups=[]
    for case in ("sat","unsat"):
        for mode in ("sequential","parallel"):
            for learning in (False,True):
                subset=[r for r in rows if (r["case"],r["mode"],r["learning"])==(case,mode,learning)]
                groups.append({"case":case,"mode":mode,"learning":learning,
                               "median_reasoning_ns":statistics.median(r["reasoning_ns"] for r in subset),
                               "median_constructor_ns":statistics.median(r["constructor_ns"] for r in subset),
                               "median_ontology_clone_ns":statistics.median(r["ontology_clone_ns"] for r in subset)})
    result={"semantic_mismatches":len(failures),"disabled_learning_violations":len(learning_violations),
            "samples":len(rows),"promotable":False,"groups":groups}
    (args.output/"summary.json").write_text(json.dumps(result,sort_keys=True,indent=2)+"\n")
    print(json.dumps(result,sort_keys=True))
    return 1 if failures or (args.assert_no_learning and learning_violations) else 0


if __name__=="__main__":
    raise SystemExit(run())
