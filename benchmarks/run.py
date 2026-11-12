#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 AG064
"""Run coding agents on the same tasks and record what happened.

Each attempt gets a fresh copy of a task's fixture. The checks stay out of the
agent's reach until it has finished. Nothing is uploaded by this script.
"""

import argparse
import json
import os
import pathlib
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

HERE = pathlib.Path(__file__).resolve().parent
TAIL_BYTES = 4000


def tasks(selected):
    found = sorted(path for path in (HERE / "tasks").iterdir() if (path / "task.json").exists())
    if selected:
        found = [path for path in found if path.name in selected]
    return found


def substitute(command, values):
    result = []
    for part in command:
        for key, value in values.items():
            part = part.replace("{" + key + "}", value)
        while "{env:" in part:
            start = part.index("{env:")
            end = part.index("}", start)
            name = part[start + 5 : end]
            if name not in os.environ:
                raise KeyError(name)
            part = part[:start] + os.environ[name] + part[end + 1 :]
        result.append(part)
    return result


def resolve(command):
    found = shutil.which(command[0])
    if not found:
        raise FileNotFoundError(command[0])
    return [found, *command[1:]]


def kill_tree(process):
    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/T", "/F", "/PID", str(process.pid)], capture_output=True, check=False
        )
    else:
        os.killpg(process.pid, 9)


def run_process(command, cwd, timeout, log=None, terminal=False):
    if terminal:
        # A step that needs the owner at a real console inherits this one.
        return subprocess.run(command, cwd=cwd, timeout=timeout, check=False).returncode, False
    options = {} if os.name == "nt" else {"start_new_session": True}
    output = open(log, "ab") if log else subprocess.DEVNULL
    stdin = subprocess.DEVNULL
    try:
        process = subprocess.Popen(
            command, cwd=cwd, stdin=stdin, stdout=output, stderr=subprocess.STDOUT, **options
        )
        try:
            return process.wait(timeout=timeout), False
        except subprocess.TimeoutExpired:
            kill_tree(process)
            process.wait()
            return None, True
    finally:
        if log:
            output.close()


def git(workspace, *arguments):
    command = ["git", "-c", "user.name=benchmark", "-c", "user.email=benchmark@example.invalid",
               "-c", "core.autocrlf=false", "-c", "commit.gpgsign=false", *arguments]
    return subprocess.run(command, cwd=workspace, capture_output=True, text=True, check=True).stdout


def changes(workspace):
    git(workspace, "add", "-A")
    files = lines = 0
    for row in git(workspace, "diff", "--cached", "--numstat").splitlines():
        added, removed, _ = row.split("\t", 2)
        files += 1
        lines += int(added) if added.isdigit() else 0
        lines += int(removed) if removed.isdigit() else 0
    return files, lines


def verify(task_path, workspace):
    shutil.copytree(task_path / "verify", workspace, dirs_exist_ok=True)
    code, timed_out = run_process(
        [sys.executable, "-m", "unittest", "discover", "-p", "test_hidden.py"],
        workspace,
        120,
    )
    return code == 0 and not timed_out


def attempt(agent, config, task_path, model, attempt_number, raw_dir):
    meta = json.loads((task_path / "task.json").read_text(encoding="utf-8"))
    record = {
        "agent": agent,
        "task": task_path.name,
        "category": meta["category"],
        "attempt": attempt_number,
        "model": model,
        "passed": False,
        "status": "ran",
        "seconds": None,
        "exit_code": None,
        "files_changed": 0,
        "lines_changed": 0,
    }
    with tempfile.TemporaryDirectory(prefix="agent-bench-") as temporary:
        base = pathlib.Path(temporary)
        workspace, state = base / "workspace", base / "state"
        state.mkdir()
        shutil.copytree(task_path / "fixture", workspace)
        git(workspace, "init", "-q")
        git(workspace, "add", "-A")
        git(workspace, "commit", "-q", "-m", "fixture")
        values = {
            "workspace": str(workspace),
            "state": str(state),
            "model": model,
            "prompt": (task_path / "prompt.md").read_text(encoding="utf-8").strip(),
        }
        started = time.monotonic()
        if agent == "null":
            code, timed_out = 0, False
        elif agent == "reference":
            shutil.copytree(task_path / "solution", workspace, dirs_exist_ok=True)
            code, timed_out = 0, False
        else:
            log = raw_dir / f"{agent}-{task_path.name}-{attempt_number}.log" if raw_dir else None
            try:
                for step in config.get("setup", []):
                    terminal = isinstance(step, dict)
                    command = step["command"] if terminal else step
                    step_code, step_timed_out = run_process(
                        resolve(substitute(command, values)), workspace, 120, log, terminal
                    )
                    if step_code != 0 or step_timed_out:
                        record["status"] = "setup failed"
                        return record
                command = resolve(substitute(config["run"], values))
            except FileNotFoundError as error:
                record["status"] = f"not installed: {error}"
                return record
            except KeyError as error:
                record["status"] = f"missing environment variable: {error}"
                return record
            code, timed_out = run_process(command, workspace, meta["timeout_seconds"], log)
        record["seconds"] = round(time.monotonic() - started, 2)
        record["exit_code"] = code
        if timed_out:
            record["status"] = "timed out"
        record["files_changed"], record["lines_changed"] = changes(workspace)
        record["passed"] = verify(task_path, workspace)
    return record


def version(config):
    if "version" not in config:
        return "built in"
    try:
        result = subprocess.run(
            resolve(config["version"]), capture_output=True, text=True, timeout=30, check=False
        )
        return (result.stdout or result.stderr).strip().splitlines()[0][:80]
    except (FileNotFoundError, IndexError, subprocess.TimeoutExpired):
        return "not installed"


def report(records, versions, model):
    agents = list(dict.fromkeys(record["agent"] for record in records))
    task_names = list(dict.fromkeys(record["task"] for record in records))
    lines = [f"Model: `{model}`", "", "| Agent | Version | Passed | Median time, passed runs |", "| --- | --- | --- | --- |"]
    for agent in agents:
        rows = [record for record in records if record["agent"] == agent]
        passed = [record for record in rows if record["passed"]]
        times = [record["seconds"] for record in passed if record["seconds"] is not None]
        median = f"{statistics.median(times):.1f} s" if times else "n/a"
        lines.append(f"| {agent} | {versions[agent]} | {len(passed)}/{len(rows)} | {median} |")
    lines += ["", "| Task | " + " | ".join(agents) + " |", "| --- |" + " --- |" * len(agents)]
    for task in task_names:
        cells = []
        for agent in agents:
            rows = [r for r in records if r["agent"] == agent and r["task"] == task]
            cells.append(f"{sum(r['passed'] for r in rows)}/{len(rows)}")
        lines.append(f"| {task} | " + " | ".join(cells) + " |")
    problems = sorted({f"{r['agent']}: {r['status']}" for r in records if r["status"] != "ran"})
    if problems:
        lines += ["", "Not a clean run:", *[f"- {problem}" for problem in problems]]
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--agents", nargs="+", required=True, help="names from agents.json, or null and reference")
    parser.add_argument("--model", default="unset", help="model id passed to every agent")
    parser.add_argument("--tasks", nargs="*", help="task names, default all")
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--config", type=pathlib.Path, default=HERE / "agents.json")
    parser.add_argument("--output", type=pathlib.Path, default=HERE / "results" / time.strftime("%Y%m%d-%H%M%S"))
    arguments = parser.parse_args()

    configured = json.loads(arguments.config.read_text(encoding="utf-8"))
    configured.update({"null": {}, "reference": {}})
    unknown = [agent for agent in arguments.agents if agent not in configured]
    if unknown:
        parser.error(f"unknown agents: {', '.join(unknown)}")

    arguments.output.mkdir(parents=True, exist_ok=True)
    raw = arguments.output / "raw"
    raw.mkdir(exist_ok=True)
    versions = {agent: version(configured[agent]) for agent in arguments.agents}
    records = []
    for agent in arguments.agents:
        for task_path in tasks(arguments.tasks):
            for number in range(1, arguments.repeats + 1):
                record = attempt(agent, configured[agent], task_path, arguments.model, number, raw)
                records.append(record)
                print(f"{agent:<12} {task_path.name:<22} #{number} "
                      f"{'pass' if record['passed'] else 'fail'}  {record['status']}", flush=True)
    (arguments.output / "results.json").write_text(
        json.dumps({"model": arguments.model, "versions": versions, "records": records}, indent=2) + "\n",
        encoding="utf-8",
    )
    summary = report(records, versions, arguments.model)
    (arguments.output / "report.md").write_text(summary, encoding="utf-8")
    print("\n" + summary)
    print(f"Saved to {arguments.output}")


if __name__ == "__main__":
    main()
