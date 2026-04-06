/**
 * Mesh Workflows Feature
 *
 * Goal decomposition with dependency graph, parallel/sequential execution,
 * checkpoint/resume integration, and progress reporting.
 */

import jsep from 'jsep';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

/**
 * Mesh Workflows feature — goal decomposition and multi-step orchestration.
 *
 * Decomposes goals into subtasks, builds dependency graphs,
 * executes steps with checkpoint/resume, and reports progress.
 */

/** Mesh workflow configuration */
export interface MeshWorkflowsConfig {
  enabled: boolean;
  maxConcurrent: number;
  defaultTimeout: number;
  persistState: boolean;
  checkpointDir: string;
}

/** Workflow step definition */
export interface WorkflowStep {
  id: string;
  name: string;
  type: 'action' | 'condition' | 'parallel' | 'loop' | 'delay' | 'agent';
  config: Record<string, unknown>;
  dependencies: string[]; // step IDs this depends on
  nextSteps: string[];
  errorStep?: string;
  timeout?: number;
}

/** Workflow definition */
export interface Workflow {
  id: string;
  name: string;
  description: string;
  goal: string;
  version: string;
  steps: WorkflowStep[];
  entryStep: string;
  variables: Record<string, unknown>;
  createdAt: number;
  updatedAt: number;
}

/** Workflow execution state */
export interface WorkflowExecution {
  id: string;
  workflowId: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'paused';
  currentStep: string;
  completedSteps: string[];
  failedSteps: string[];
  variables: Record<string, unknown>;
  stepResults: Map<string, unknown>;
  progress: number; // 0-100
  startedAt: number;
  completedAt?: number;
  error?: string;
  checkpointData?: unknown;
}

/** Step handler function */
export type StepHandler = (
  step: WorkflowStep,
  context: Record<string, unknown>,
  execution: WorkflowExecution,
) => Promise<unknown>;

/** Goal decomposition result */
export interface DecomposedGoal {
  goal: string;
  subtasks: Array<{
    id: string;
    name: string;
    description: string;
    dependencies: string[];
    estimatedDuration?: number;
  }>;
  executionPlan: string[];
}

// Evaluate parsed AST node
function evalNode(node: any, vars: Record<string, unknown>): any {
  switch (node.type) {
    case 'BinaryExpression': {
      const left = evalNode(node.left, vars);
      const right = evalNode(node.right, vars);
      switch (node.operator) {
        case '+':
          return left + right;
        case '-':
          return left - right;
        case '*':
          return left * right;
        case '/':
          return left / right;
        case '>':
          return left > right;
        case '<':
          return left < right;
        case '>=':
          return left >= right;
        case '<=':
          return left <= right;
        case '==':
          return left == right;
        case '!=':
          return left != right;
        case '===':
          return left === right;
        case '!==':
          return left !== right;
      }
      // Handle LogicalExpression operators in BinaryExpression case (jsep quirk)
      if (node.operator === '&&') return evalNode(node.left, vars) && evalNode(node.right, vars);
      if (node.operator === '||') return evalNode(node.left, vars) || evalNode(node.right, vars);
      throw new Error(`Unsupported operator: ${node.operator}`);
    }
    case 'LogicalExpression': {
      if (node.operator === '&&') return evalNode(node.left, vars) && evalNode(node.right, vars);
      if (node.operator === '||') return evalNode(node.left, vars) || evalNode(node.right, vars);
      throw new Error(`Unsupported logical operator: ${node.operator}`);
    }
    case 'UnaryExpression': {
      const val = evalNode(node.argument, vars);
      if (node.operator === '!') return !val;
      if (node.operator === '+') return +val;
      if (node.operator === '-') return -val;
      throw new Error(`Unsupported unary operator: ${node.operator}`);
    }
    case 'Identifier': {
      const name = node.name;
      if (Object.prototype.hasOwnProperty.call(vars, name)) return vars[name as keyof typeof vars];
      return undefined;
    }
    case 'Literal':
      return node.value;
    case 'MemberExpression': {
      const obj = evalNode(node.object, vars);
      const prop = node.computed ? evalNode(node.property, vars) : node.property.name;
      return obj ? obj[prop] : undefined;
    }
    case 'Compound':
      // Handle multiple expressions (e.g., from jsep parsing)
      if (Array.isArray(node.body)) {
        return node.body.reduce((_acc: unknown, n: { type: string }) => evalNode(n, vars), undefined);
      }
      return evalNode(node.body, vars);
    default:
      throw new Error(`Unsupported node type: ${(node as { type: string }).type}`);
  }
}

function evaluateCondition(condition: string, vars: Record<string, unknown> = {}): boolean {
  if (!condition || typeof condition !== 'string') return false;
  const ast = jsep(condition);
  const val = evalNode(ast, vars);
  return !!val;
}

class MeshWorkflowsFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'mesh-workflows',
    version: '0.2.0',
    description: 'Goal decomposition, dependency graph, checkpoint/resume workflows',
    dependencies: ['checkpoint'],
  };

  private config: MeshWorkflowsConfig = {
    enabled: false,
    maxConcurrent: 10,
    defaultTimeout: 300000,
    persistState: true,
    checkpointDir: './data/checkpoints',
  };
  private ctx!: FeatureContext;
  private workflows: Map<string, Workflow> = new Map();
  private executions: Map<string, WorkflowExecution> = new Map();
  private stepHandlers: Map<string, StepHandler> = new Map();
  private runningCount = 0;
  private progressListeners: Array<(exec: WorkflowExecution) => void> = [];

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<MeshWorkflowsConfig>) };
  }

  async start(): Promise<void> {
    // Register built-in handlers
    this.registerStepHandler('delay', async (step) => {
      const ms = (step.config['duration'] as number) ?? 1000;
      await new Promise((r) => setTimeout(r, ms));
      return { delayed: ms };
    });
    this.registerStepHandler('condition', async (step, vars) => {
      const condition = step.config['condition'] as string;
      // Safe condition evaluation using jsep
      try {
        const result = evaluateCondition(condition, vars);
        return { result: !!result, nextStep: result ? step.nextSteps[0] : step.nextSteps[1] };
      } catch (err) {
        this.ctx.logger.warn('Condition evaluation failed', {
          condition,
          error: err instanceof Error ? err.message : String(err),
        });
        return { result: false };
      }
    });
    this.ctx.logger.info('Mesh Workflows active', { maxConcurrent: this.config.maxConcurrent });
  }

  async stop(): Promise<void> {
    for (const [, exec] of this.executions) {
      if (exec.status === 'running') exec.status = 'paused';
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    const running = Array.from(this.executions.values()).filter(
      (e) => e.status === 'running',
    ).length;
    return {
      healthy: running < this.config.maxConcurrent,
      details: {
        workflows: this.workflows.size,
        executions: this.executions.size,
        running,
        handlers: this.stepHandlers.size,
      },
    };
  }

  // ─── Goal Decomposition ───────────────────────────────────────────────────

  /** Decompose a goal into subtasks using the agent */
  async decomposeGoal(
    goal: string,
    context: Record<string, unknown> = {},
  ): Promise<DecomposedGoal> {
    // Analyze goal and create subtasks
    const subtasks = this.analyzeGoal(goal, context);

    // Build execution plan respecting dependencies
    const executionPlan = this.topologicalSort(subtasks);

    return { goal, subtasks, executionPlan };
  }

  /** Simple goal analysis — extracts subtasks from goal description */
  private analyzeGoal(
    goal: string,
    __context: Record<string, unknown>,
  ): DecomposedGoal['subtasks'] {
    const subtasks: DecomposedGoal['subtasks'] = [];
    const lines = goal
      .split(/[.\n]/)
      .map((l) => l.trim())
      .filter(Boolean);

    // If goal has explicit steps (numbered or bulleted), use them
    const explicitSteps = lines.filter((l) => /^\d+[\.\)]\s/.test(l) || /^[-*]\s/.test(l));
    if (explicitSteps.length > 1) {
      explicitSteps.forEach((step, i) => {
        const name = step.replace(/^(\d+[\.\)]|[-*])\s*/, '').trim();
        subtasks.push({
          id: `step_${i + 1}`,
          name,
          description: name,
          dependencies: i > 0 ? [`step_${i}`] : [],
        });
      });
    } else {
      // Default decomposition: research -> plan -> execute -> verify
      subtasks.push(
        {
          id: 'research',
          name: 'Research & gather info',
          description: `Research context for: ${goal}`,
          dependencies: [],
        },
        {
          id: 'plan',
          name: 'Create execution plan',
          description: 'Plan approach and resources',
          dependencies: ['research'],
        },
        { id: 'execute', name: 'Execute main task', description: goal, dependencies: ['plan'] },
        {
          id: 'verify',
          name: 'Verify results',
          description: 'Check that goal is achieved',
          dependencies: ['execute'],
        },
      );
    }

    return subtasks;
  }

  /** Topological sort for dependency ordering */
  private topologicalSort(subtasks: DecomposedGoal['subtasks']): string[] {
    const inDegree = new Map<string, number>();
    const adjacency = new Map<string, string[]>();
    const ids = subtasks.map((s) => s.id);

    for (const id of ids) {
      inDegree.set(id, 0);
      adjacency.set(id, []);
    }
    for (const task of subtasks) {
      for (const dep of task.dependencies) {
        if (adjacency.has(dep)) {
          adjacency.get(dep)!.push(task.id);
          inDegree.set(task.id, (inDegree.get(task.id) ?? 0) + 1);
        }
      }
    }

    const queue = ids.filter((id) => (inDegree.get(id) ?? 0) === 0);
    const order: string[] = [];
    while (queue.length > 0) {
      const id = queue.shift()!;
      order.push(id);
      for (const neighbor of adjacency.get(id) ?? []) {
        inDegree.set(neighbor, (inDegree.get(neighbor) ?? 1) - 1);
        if ((inDegree.get(neighbor) ?? 0) === 0) queue.push(neighbor);
      }
    }
    return order;
  }

  // ─── Workflow Management ──────────────────────────────────────────────────

  /** Register a workflow definition */
  registerWorkflow(workflow: Omit<Workflow, 'createdAt' | 'updatedAt'>): Workflow {
    const now = Date.now();
    const full: Workflow = { ...workflow, createdAt: now, updatedAt: now };
    this.workflows.set(workflow.id, full);
    this.ctx.logger.info('Workflow registered', {
      id: workflow.id,
      name: workflow.name,
      steps: workflow.steps.length,
    });
    return full;
  }

  /** Create a workflow from a decomposed goal */
  createWorkflowFromGoal(goalId: string, decomposed: DecomposedGoal): Workflow {
    const steps: WorkflowStep[] = decomposed.subtasks.map((task) => ({
      id: task.id,
      name: task.name,
      type: 'agent' as const,
      config: { description: task.description, goal: decomposed.goal },
      dependencies: task.dependencies,
      nextSteps: [],
      timeout: this.config.defaultTimeout,
    }));

    // Set nextSteps based on execution plan order
    for (let i = 0; i < decomposed.executionPlan.length - 1; i++) {
      const step = steps.find((s) => s.id === decomposed.executionPlan[i]);
      if (step) step.nextSteps = [decomposed.executionPlan[i + 1]!];
    }

    return this.registerWorkflow({
      id: goalId,
      name: `Goal: ${decomposed.goal.slice(0, 50)}`,
      description: decomposed.goal,
      goal: decomposed.goal,
      version: '1.0.0',
      steps,
      entryStep: decomposed.executionPlan[0] ?? '',
      variables: {},
    });
  }

  /** Register a step handler */
  registerStepHandler(type: string, handler: StepHandler): void {
    this.stepHandlers.set(type, handler);
  }

  /** Register a progress listener */
  onProgress(listener: (exec: WorkflowExecution) => void): void {
    this.progressListeners.push(listener);
  }

  // ─── Execution ────────────────────────────────────────────────────────────

  /** Execute a workflow */
  async execute(
    workflowId: string,
    variables: Record<string, unknown> = {},
  ): Promise<WorkflowExecution> {
    if (this.runningCount >= this.config.maxConcurrent)
      throw new Error('Maximum concurrent executions reached');

    const workflow = this.workflows.get(workflowId);
    if (!workflow) throw new Error(`Workflow not found: ${workflowId}`);

    const executionId = `exec_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const execution: WorkflowExecution = {
      id: executionId,
      workflowId,
      status: 'running',
      currentStep: workflow.entryStep,
      completedSteps: [],
      failedSteps: [],
      variables: { ...workflow.variables, ...variables },
      stepResults: new Map(),
      progress: 0,
      startedAt: Date.now(),
    };

    this.executions.set(executionId, execution);
    this.runningCount++;

    // Run asynchronously with dependency awareness
    this.runExecution(execution, workflow).catch((err) => {
      this.ctx.logger.error('Workflow execution error', {
        executionId,
        error: err instanceof Error ? err.message : String(err),
      });
    });

    return execution;
  }

  /** Run execution with dependency-aware scheduling */
  private async runExecution(execution: WorkflowExecution, workflow: Workflow): Promise<void> {
    const completed = new Set<string>();
    const failed = new Set<string>();
    const totalSteps = workflow.steps.length;

    try {
      while (execution.status === 'running' && completed.size < totalSteps) {
        // Find steps ready to run (dependencies satisfied)
        const readySteps = workflow.steps.filter(
          (step) =>
            !completed.has(step.id) &&
            !failed.has(step.id) &&
            step.dependencies.every((d) => completed.has(d)),
        );

        if (readySteps.length === 0) {
          // Check if we're stuck (remaining steps have failed dependencies)
          const remaining = workflow.steps.filter((s) => !completed.has(s.id) && !failed.has(s.id));
          if (remaining.length > 0) {
            execution.status = 'failed';
            execution.error = 'Deadlock: remaining steps have unsatisfied dependencies';
          }
          break;
        }

        // Run ready steps (parallel if multiple)
        await Promise.all(
          readySteps.map(async (step) => {
            execution.currentStep = step.id;
            this.emitProgress(execution);

            try {
              const handler = this.stepHandlers.get(step.type);
              if (handler) {
                const result = await handler(step, execution.variables, execution);
                execution.stepResults.set(step.id, result);

                // Handle condition branching
                if (step.type === 'condition' && result && typeof result === 'object') {
                  const r = result as { nextStep?: string };
                  if (r.nextStep) step.nextSteps = [r.nextStep];
                }
              }
              completed.add(step.id);
              execution.completedSteps.push(step.id);
            } catch (err) {
              failed.add(step.id);
              execution.failedSteps.push(step.id);
              this.ctx.logger.error('Step failed', {
                step: step.name,
                error: err instanceof Error ? err.message : String(err),
              });

              if (step.errorStep) {
                // Execute error handler step
                const errorStep = workflow.steps.find((s) => s.id === step.errorStep);
                if (errorStep) {
                  try {
                    const handler = this.stepHandlers.get(errorStep.type);
                    if (handler) await handler(errorStep, execution.variables, execution);
                  } catch {}
                }
              }
            }

            // Update progress
            execution.progress = Math.round((completed.size / totalSteps) * 100);
            this.emitProgress(execution);
          }),
        );
      }

      if (execution.status === 'running') {
        execution.status = completed.size === totalSteps ? 'completed' : 'failed';
      }
    } finally {
      execution.completedAt = Date.now();
      this.runningCount--;
      this.emitProgress(execution);
    }
  }

  /** Emit progress to listeners */
  private emitProgress(exec: WorkflowExecution): void {
    for (const listener of this.progressListeners) {
      try {
        listener(exec);
      } catch {}
    }
  }

  /** Get execution status */
  getExecution(executionId: string): WorkflowExecution | undefined {
    return this.executions.get(executionId);
  }

  /** Pause execution */
  pauseExecution(executionId: string): boolean {
    const exec = this.executions.get(executionId);
    if (exec?.status === 'running') {
      exec.status = 'paused';
      return true;
    }
    return false;
  }

  /** Resume paused execution */
  async resumeExecution(executionId: string): Promise<boolean> {
    const exec = this.executions.get(executionId);
    if (exec?.status === 'paused') {
      exec.status = 'running';
      const workflow = this.workflows.get(exec.workflowId);
      if (workflow) this.runExecution(exec, workflow);
      return true;
    }
    return false;
  }

  /** Checkpoint current execution state */
  checkpointExecution(executionId: string): unknown {
    const exec = this.executions.get(executionId);
    if (!exec) return null;
    return {
      id: exec.id,
      workflowId: exec.workflowId,
      status: exec.status,
      currentStep: exec.currentStep,
      completedSteps: [...exec.completedSteps],
      variables: { ...exec.variables },
      stepResults: Array.from(exec.stepResults.entries()),
      progress: exec.progress,
      startedAt: exec.startedAt,
    };
  }

  /** Restore execution from checkpoint */
  async restoreFromCheckpoint(data: Record<string, unknown>): Promise<WorkflowExecution> {
    const workflowId = data['workflowId'] as string;
    const workflow = this.workflows.get(workflowId);
    if (!workflow) throw new Error(`Workflow not found: ${workflowId}`);

    const execution: WorkflowExecution = {
      id: data['id'] as string,
      workflowId,
      status: 'paused',
      currentStep: data['currentStep'] as string,
      completedSteps: (data['completedSteps'] as string[]) ?? [],
      failedSteps: [],
      variables: (data['variables'] as Record<string, unknown>) ?? {},
      stepResults: new Map(data['stepResults'] as [string, unknown][]),
      progress: data['progress'] as number,
      startedAt: data['startedAt'] as number,
      checkpointData: data,
    };

    this.executions.set(execution.id, execution);
    return execution;
  }
}

export default new MeshWorkflowsFeature();
