"""Pier agents for tools Pier does not ship: Argentum, Pi, and DeepSeek Harness.

Each one runs inside the task's container. The task's own instruction tells every agent to
commit its work, and nothing here commits for it, so all agents are graded the same way.
Keys come from the environment and are never written to a file or a command line.
"""

import os
import shlex

from pier.agents.installed.base import BaseInstalledAgent, with_prompt_template
from pier.environments.base import BaseEnvironment
from pier.models.agent.context import AgentContext
from pier.models.agent.install import AgentInstallSpec, InstallStep
from pier.models.agent.network import NetworkAllowlist

DEEPSEEK_ONLY = NetworkAllowlist(domains=["api.deepseek.com"])
NODE = (
    "curl -fsSL https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash"
    " && . ~/.nvm/nvm.sh && nvm install 22"
)
APT_CURL = "apt-get update && apt-get install -y curl"


def model_id(name):
    return name.split("/", 1)[-1] if name else "deepseek-flash"


def deepseek_env(agent):
    key = agent._get_env("DEEPSEEK_API_KEY")
    if not key:
        raise ValueError("DEEPSEEK_API_KEY is not set")
    return {"DEEPSEEK_API_KEY": key}


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
        return DEEPSEEK_ONLY

    def install_spec(self):
        return AgentInstallSpec(
            agent_name=self.name(),
            steps=[InstallStep(user="root", run="chmod 755 /usr/local/bin/argentum")],
            verification_command="argentum --version",
        )

    async def install(self, environment: BaseEnvironment):
        binary = os.environ["ARGENTUM_BENCHMARK_BINARY"]
        await environment.upload_file(binary, "/usr/local/bin/argentum")
        await super().install(environment)

    @with_prompt_template
    async def run(self, instruction, environment, context: AgentContext):
        env = {**deepseek_env(self), "ARGENTUM_BENCHMARK_DISPOSABLE": "1", "RUST_LOG": "off"}
        model = shlex.quote(model_id(self.model_name))
        await self.exec_as_agent(
            environment,
            command=(
                f'argentum benchmark-run --workspace "$PWD" --model {model}'
                f" --prompt {shlex.quote(instruction)} > /logs/agent/argentum.jsonl 2>&1"
            ),
            env=env,
        )

    def populate_context_post_run(self, context: AgentContext):
        return None


class Pi(BaseInstalledAgent):
    """Pi, installed the way Harbor's own integration installs it."""

    @staticmethod
    def name():
        return "pi"

    def get_version_command(self):
        return ". ~/.nvm/nvm.sh; pi --version"

    def network_allowlist(self):
        return DEEPSEEK_ONLY

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
        model = shlex.quote(model_id(self.model_name))
        await self.exec_as_agent(
            environment,
            command=(
                f". ~/.nvm/nvm.sh; pi --print --provider deepseek --model {model}"
                f" {shlex.quote(instruction)} > /logs/agent/pi.txt 2>&1"
            ),
            env=deepseek_env(self),
        )

    def populate_context_post_run(self, context: AgentContext):
        return None


class DeepSeekHarness(BaseInstalledAgent):
    """DeepSeek Harness in its documented one-shot mode.

    It uploads session logs to DeepSeek by default, so only run it on these public tasks.
    """

    @staticmethod
    def name():
        return "deepseek-harness"

    def get_version_command(self):
        return ". ~/.nvm/nvm.sh; dsh --version"

    def network_allowlist(self):
        return DEEPSEEK_ONLY

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
        env = {**deepseek_env(self), "DSH_PERMISSION_MODE": "danger-full-access"}
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
