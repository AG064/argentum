#!/usr/bin/env python3
"""Run DeepSWE on one model across agent harnesses, through Datacurve's Pier.

Prints the commands by default and spends nothing. Add --go to run them. Run it where
Docker works, or pass --env modal. Keys come from the environment and never reach a
command line.
"""

import argparse
import os
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
AGENTS_MODULE = "agents"
GO_BASE = "https://opencode.ai/inference/go"
DEEPSEEK_BASE = "https://api.deepseek.com"
# What each route is called by the tools, and where its key lives.
ROUTES = {"opencode-go": ("deepseek-v4.1-flash", "OPENCODE_API_KEY"), "deepseek": ("deepseek-flash", "DEEPSEEK_API_KEY")}


# DeepSWE labels every result with its reasoning effort, and its leaderboard tests models
# at their highest. Every harness asks for the same one. Argentum and Pi set it in agents.py.
MAX_EFFORT = ["--ak", "reasoning_effort=max"]
# Sends session logs to DeepSeek by default, which DeepSWE's data rule does not allow.
NEEDS_LOG_UPLOAD_OFF = {"deepseek-harness"}


def custom(name, model):
    return ["--agent-import-path", f"{AGENTS_MODULE}:{name}", "--model", model]


def harnesses(route, model):
    """Every agent gets the same model. Only how each one reaches it differs."""
    key = os.environ.get(ROUTES[route][1], "")
    routed = f"{route}/{model}"
    if route == "opencode-go":
        return {
            # The control. It is not on OpenCode's validated list, so check it first.
            "mini-swe-agent": (
                ["--agent", "mini-swe-agent", "--model", f"openai/{model}", *MAX_EFFORT],
                {"OPENAI_API_BASE": f"{GO_BASE}/openai/v1", "OPENAI_API_KEY": key},
            ),
            "opencode": ([*custom("OpenCodeGo", routed), "--ak", "variant=max"], {}),
            "claude-code": (
                ["--agent", "claude-code", "--model", model, *MAX_EFFORT],
                {"ANTHROPIC_BASE_URL": f"{GO_BASE}/anthropic", "ANTHROPIC_AUTH_TOKEN": key},
            ),
            "codex": (
                ["--agent", "codex", "--model", model, *MAX_EFFORT],
                {"OPENAI_BASE_URL": f"{GO_BASE}/openai/v1", "OPENAI_API_KEY": key},
            ),
            "argentum": (custom("Argentum", routed), {}),
            "pi": (custom("Pi", routed), {}),
        }
    return {
        "mini-swe-agent": (["--agent", "mini-swe-agent", "--model", f"deepseek/{model}", *MAX_EFFORT], {}),
        "opencode": (["--agent", "opencode", "--model", f"deepseek/{model}", "--ak", "variant=max"], {}),
        "claude-code": (
            ["--agent", "claude-code", "--model", model, *MAX_EFFORT],
            {"ANTHROPIC_BASE_URL": f"{DEEPSEEK_BASE}/anthropic", "ANTHROPIC_API_KEY": key},
        ),
        "codex": (
            ["--agent", "codex", "--model", model, *MAX_EFFORT],
            {"OPENAI_BASE_URL": DEEPSEEK_BASE, "OPENAI_API_KEY": key},
        ),
        "argentum": (custom("Argentum", routed), {}),
        "pi": (custom("Pi", routed), {}),
        "deepseek-harness": (custom("DeepSeekHarness", routed), {}),
    }


def commands(args):
    table = harnesses(args.route, args.model)
    blocked = NEEDS_LOG_UPLOAD_OFF.intersection(args.agents)
    if blocked:
        raise SystemExit(f"{', '.join(sorted(blocked))} sends session logs by default: see the README")
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
    parser.add_argument("--route", choices=ROUTES, default="opencode-go")
    parser.add_argument("--model", help="the route's DeepSeek Flash model by default")
    parser.add_argument("--agents", nargs="+", help="every agent on the route by default")
    parser.add_argument("--n-tasks", type=int, default=20, help="0 runs all 113")
    parser.add_argument("--seed", type=int, default=0, help="the same seed gives every agent the same tasks")
    parser.add_argument("--concurrent", type=int, default=1, help="each task can use 8 GB of memory")
    parser.add_argument("--env", help="for example modal. Local Docker by default")
    parser.add_argument("--go", action="store_true", help="really run, and spend")
    args = parser.parse_args(argv)
    args.model = args.model or ROUTES[args.route][0]
    args.agents = args.agents or [
        name for name in harnesses(args.route, args.model) if name not in NEEDS_LOG_UPLOAD_OFF
    ]
    key_variable = ROUTES[args.route][1]

    for name, command, env in commands(args):
        print(f"[{name}] {' '.join(command)}")
        print(f"[{name}] extra environment: {', '.join(env) or 'none'}")
        if not args.go:
            continue
        if not os.environ.get(key_variable):
            raise SystemExit(f"{key_variable} is not set")
        python_path = os.pathsep.join(filter(None, [str(HERE), os.environ.get("PYTHONPATH")]))
        code = subprocess.call(command, env={**os.environ, **env, "PYTHONPATH": python_path})
        print(f"[{name}] exit code {code}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
