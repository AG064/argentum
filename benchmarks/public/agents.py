# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 AG064

"""Pier agents for tools Pier does not ship: Argentum, Pi, and DeepSeek Harness.

Each one runs inside the task's container. The task's own instruction tells every agent to
commit its work, and nothing here commits for it, so all agents are graded the same way.
Keys come from the environment and are never written to a file or a command line.
"""

import os
import shlex

from pier.agents.installed.base import BaseInstalledAgent, with_prompt_template
from pier.agents.installed.opencode import OpenCode
from pier.environments.base import BaseEnvironment
from pier.models.agent.context import AgentContext
from pier.models.agent.install import AgentInstallSpec, InstallStep
from pier.models.agent.network import NetworkAllowlist

ROUTES = {
    "deepseek": ("DEEPSEEK_API_KEY", NetworkAllowlist(domains=["api.deepseek.com"])),
    "opencode-go": ("OPENCODE_API_KEY", NetworkAllowlist(domains=["opencode.ai"])),
}
NODE = (
    "curl -fsSL https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash"
    " && . ~/.nvm/nvm.sh && nvm install 22"
)
APT_CURL = "apt-get update && apt-get install -y curl"


def route_of(name):
    """`opencode-go/deepseek-v4.1-flash` gives the route and the model ID."""
    route, _, model = (name or "").partition("/")
    if route not in ROUTES or not model:
        raise ValueError(f"model must be one of {', '.join(ROUTES)}, then /MODEL: {name}")
    return route, model


def key_env(agent):
    variable = ROUTES[route_of(agent.model_name)[0]][0]
    key = agent._get_env(variable)
    if not key:
        raise ValueError(f"{variable} is not set")
    return {variable: key}


def allowlist(agent):
    return ROUTES[route_of(agent.model_name)[0]][1]


class Argentum(BaseInstalledAgent):
    """The benchmark build of the Argentum CLI, copied in from this machine.

    ARGENTUM_BENCHMARK_BINARY points at a build made with the `benchmark-unattended`
    feature. Ordinary builds do not have the `benchmark-run` command.
    """

    @staticmethod
    def name():
        return "argentum"

    def get_version_command(self):
        return "argentum --version"

    def network_allowlist(self):
        return allowlist(self)

    def install_spec(self):
        # Pier bakes these steps into the image before a trial, when the binary is not
        # there yet, so the binary is copied in by setup below.
        return AgentInstallSpec(
            agent_name=self.name(),
            steps=[InstallStep(user="root", run="mkdir -p /usr/local/bin")],
        )

    async def setup(self, environment: BaseEnvironment):
        await super().setup(environment)
        binary = os.environ["ARGENTUM_BENCHMARK_BINARY"]
        await environment.upload_file(binary, "/usr/local/bin/argentum")
        await self.exec_as_root(environment, command="chmod 755 /usr/local/bin/argentum")

    @with_prompt_template
    async def run(self, instruction, environment, context: AgentContext):
        env = {**key_env(self), "ARGENTUM_BENCHMARK_DISPOSABLE": "1", "RUST_LOG": "off"}
        route, model = route_of(self.model_name)
        await self.exec_as_agent(
            environment,
            command=(
                f'argentum benchmark-run --workspace "$PWD" --provider {route}'
                f" --model {shlex.quote(model)} --thinking max"
                f" --prompt {shlex.quote(instruction)} > /logs/agent/argentum.jsonl 2>&1"
            ),
            env=env,
        )

    def populate_context_post_run(self, context: AgentContext):
        return None


class OpenCodeGo(OpenCode):
    """Pier's OpenCode agent, which knows the Zen key but not the Go route."""

    def network_allowlist(self):
        return allowlist(self)

    def build_process_env(self, *args, **kwargs):
        return {**super().build_process_env(*args, **kwargs), **key_env(self)}


class Pi(BaseInstalledAgent):
    """Pi, installed the way Harbor's own integration installs it."""

    @staticmethod
    def name():
        return "pi"

    def get_version_command(self):
        return ". ~/.nvm/nvm.sh; pi --version"

    def network_allowlist(self):
        return allowlist(self)

    def install_spec(self):
        return AgentInstallSpec(
            agent_name=self.name(),
            steps=[
                InstallStep(user="root", env={"DEBIAN_FRONTEND": "noninteractive"}, run=APT_CURL),
                InstallStep(
                    run=f"{NODE} && npm install -g --ignore-scripts @earendil-works/pi-coding-agent"
                ),
            ],
        )

    @with_prompt_template
    async def run(self, instruction, environment, context: AgentContext):
        route, model = route_of(self.model_name)
        await self.exec_as_agent(
            environment,
            command=(
                f". ~/.nvm/nvm.sh; pi --print --thinking max --provider {route}"
                f" --model {shlex.quote(model)}"
                f" {shlex.quote(instruction)} > /logs/agent/pi.txt 2>&1"
            ),
            env=key_env(self),
        )

    def populate_context_post_run(self, context: AgentContext):
        return None


class DeepSeekHarness(BaseInstalledAgent):
    """DeepSeek Harness in its documented one-shot mode.

    It sends session logs to DeepSeek by default. DeepSWE says its data must never reach a
    training corpus, so the runner leaves this out until that upload is proven off.
    OpenCode lists it as having incomplete session support on Go, so it uses DeepSeek directly.
    """

    @staticmethod
    def name():
        return "deepseek-harness"

    def get_version_command(self):
        return ". ~/.nvm/nvm.sh; dsh --version"

    def network_allowlist(self):
        return allowlist(self)

    def install_spec(self):
        return AgentInstallSpec(
            agent_name=self.name(),
            steps=[
                InstallStep(user="root", env={"DEBIAN_FRONTEND": "noninteractive"}, run=APT_CURL),
                InstallStep(run=f"{NODE} && npm install -g @deepseek-ai/dsh"),
            ],
        )

    @with_prompt_template
    async def run(self, instruction, environment, context: AgentContext):
        env = {**key_env(self), "DSH_PERMISSION_MODE": "danger-full-access"}
        await self.exec_as_agent(
            environment,
            command=(
                f". ~/.nvm/nvm.sh; dsh --profile headless {shlex.quote(instruction)}"
                " > /logs/agent/dsh.txt 2>&1"
            ),
            env=env,
        )

    def populate_context_post_run(self, context: AgentContext):
        return None
