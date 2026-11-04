#!/usr/bin/env python3
"""Run DeepSWE on one model across agent harnesses, through Datacurve's Pier.

Prints the commands by default and spends nothing. Add --go to run them. Run it where
Docker works, or pass --env modal. Keys come from DEEPSEEK_API_KEY and never reach a
command line.
"""

import argparse
import os
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
DEEPSEEK = "https://api.deepseek.com"
AGENTS_MODULE = "agents"


def harnesses(model):
    """Every agent gets the same model. Only how each one reaches it differs."""
    key = os.environ.get("DEEPSEEK_API_KEY", "")
    return {
        # The control: what DeepSWE's leaderboard runs every model on.
        "mini-swe-agent": (["--agent", "mini-swe-agent", "--model", f"deepseek/{model}"], {}),
        "opencode": (["--agent", "opencode", "--model", f"deepseek/{model}"], {}),
        "claude-code": (
            ["--agent", "claude-code", "--model", model],
            {"ANTHROPIC_BASE_URL": f"{DEEPSEEK}/anthropic", "ANTHROPIC_API_KEY": key},
        ),
        "codex": (
            ["--agent", "codex", "--model", model],
            {"OPENAI_BASE_URL": DEEPSEEK, "OPENAI_API_KEY": key},
        ),
        "argentum": (
            ["--agent-import-path", f"{AGENTS_MODULE}:Argentum", "--model", f"deepseek/{model}"],
            {},
        ),
        "pi": (["--agent-import-path", f"{AGENTS_MODULE}:Pi", "--model", f"deepseek/{model}"], {}),
        "deepseek-harness": (
            [
                "--agent-import-path",
                f"{AGENTS_MODULE}:DeepSeekHarness",
                "--model",
                f"deepseek/{model}",
            ],
            {},
        ),
    }


def commands(args):
    table = harnesses(args.model)
    unknown = [name for name in args.agents if name not in table]
    if unknown:
        raise SystemExit(f"unknown agents: {', '.join(unknown)}. Known: {', '.join(table)}")
    for name in args.agents:
        selection, env = table[name]
        command = ["pier", "run", "-p", str(args.tasks), *selection, "--jobs-dir", str(args.jobs / name)]
        command += ["--n-concurrent", str(args.concurrent)]
        if args.n_tasks:
            command += ["--n-tasks", str(args.n_tasks), "--sample-seed", str(args.seed)]
        if args.env:
            command += ["--env", args.env]
        yield name, command, env


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tasks", type=pathlib.Path, required=True, help="deep-swe/tasks in a clone")
    parser.add_argument("--jobs", type=pathlib.Path, default=pathlib.Path("deepswe-jobs"))
    parser.add_argument("--model", default="deepseek-flash")
    parser.add_argument("--agents", nargs="+", default=list(harnesses("x")))
    parser.add_argument("--n-tasks", type=int, default=20, help="0 runs all 113")
    parser.add_argument("--seed", type=int, default=0, help="the same seed gives every agent the same tasks")
    parser.add_argument("--concurrent", type=int, default=2)
    parser.add_argument("--env", help="for example modal. Local Docker by default")
    parser.add_argument("--go", action="store_true", help="really run, and spend")
    args = parser.parse_args(argv)

    for name, command, env in commands(args):
        print(f"[{name}] {' '.join(command)}")
        print(f"[{name}] extra environment: {', '.join(env) or 'none'}")
        if not args.go:
            continue
        if not os.environ.get("DEEPSEEK_API_KEY"):
            raise SystemExit("DEEPSEEK_API_KEY is not set")
        python_path = os.pathsep.join(filter(None, [str(HERE), os.environ.get("PYTHONPATH")]))
        code = subprocess.call(command, env={**os.environ, **env, "PYTHONPATH": python_path})
        print(f"[{name}] exit code {code}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
