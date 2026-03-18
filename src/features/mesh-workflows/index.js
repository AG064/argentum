"use strict";
/**
 * Mesh Workflows Feature
 *
 * Goal decomposition with dependency graph, parallel/sequential execution,
 * checkpoint/resume integration, and progress reporting.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
/**
 * Mesh Workflows feature — goal decomposition and multi-step orchestration.
 *
 * Decomposes goals into subtasks, builds dependency graphs,
 * executes steps with checkpoint/resume, and reports progress.
 */
const jsep_1 = __importDefault(require("jsep"));
// Evaluate parsed AST node
function evalNode(node, vars) {
    switch (node.type) {
        case 'BinaryExpression': {
            const left = evalNode(node.left, vars);
            const right = evalNode(node.right, vars);
            switch (node.operator) {
                case '+': return left + right;
                case '-': return left - right;
                case '*': return left * right;
                case '/': return left / right;
                case '>': return left > right;
                case '<': return left < right;
                case '>=': return left >= right;
                case '<=': return left <= right;
                case '==': return left == right;
                case '!=': return left != right;
                case '===': return left === right;
                case '!==': return left !== right;
            }
            throw new Error('Unsupported operator: ' + node.operator);
        }
        case 'LogicalExpression': {
            if (node.operator === '&&')
                return evalNode(node.left, vars) && evalNode(node.right, vars);
            if (node.operator === '||')
                return evalNode(node.left, vars) || evalNode(node.right, vars);
            throw new Error('Unsupported logical operator: ' + node.operator);
        }
        case 'UnaryExpression': {
            const val = evalNode(node.argument, vars);
            if (node.operator === '!')
                return !val;
            if (node.operator === '+')
                return +val;
            if (node.operator === '-')
                return -val;
            throw new Error('Unsupported unary operator: ' + node.operator);
        }
        case 'Identifier': {
            const name = node.name;
            if (Object.prototype.hasOwnProperty.call(vars, name))
                return vars[name];
            return undefined;
        }
        case 'Literal': return node.value;
        case 'MemberExpression': {
            const obj = evalNode(node.object, vars);
            const prop = node.computed ? evalNode(node.property, vars) : node.property.name;
            return obj ? obj[prop] : undefined;
        }
        default: throw new Error('Unsupported node type: ' + node.type);
    }
}
function evaluateCondition(condition, vars = {}) {
    if (!condition || typeof condition !== 'string')
        return false;
    const ast = (0, jsep_1.default)(condition);
    const val = evalNode(ast, vars);
    return !!val;
}
class MeshWorkflowsFeature {
    constructor() {
        this.meta = {
            name: 'mesh-workflows',
            version: '0.2.0',
            description: 'Goal decomposition, dependency graph, checkpoint/resume workflows',
            dependencies: ['checkpoint'],
        };
        this.config = {
            enabled: false,
            maxConcurrent: 10,
            defaultTimeout: 300000,
            persistState: true,
            checkpointDir: './data/checkpoints',
        };
        this.workflows = new Map();
        this.executions = new Map();
        this.stepHandlers = new Map();
        this.runningCount = 0;
        this.progressListeners = [];
    }
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
    }
    async start() {
        // Register built-in handlers
        this.registerStepHandler('delay', async (step) => {
            const ms = step.config.duration ?? 1000;
            await new Promise(r => setTimeout(r, ms));
            return { delayed: ms };
        });
        this.registerStepHandler('condition', async (step, vars) => {
            const condition = step.config.condition;
            // Safe condition evaluation using jsep
            try {
                const result = evaluateCondition(condition, vars);
                return { result: !!result, nextStep: result ? step.nextSteps[0] : step.nextSteps[1] };
            }
            catch (err) {
                this.ctx.logger.warn('Condition evaluation failed', { condition, error: err instanceof Error ? err.message : String(err) });
                return { result: false };
            }
        });
        this.ctx.logger.info('Mesh Workflows active', { maxConcurrent: this.config.maxConcurrent });
    }
    async stop() {
        for (const [, exec] of this.executions) {
            if (exec.status === 'running')
                exec.status = 'paused';
        }
    }
    async healthCheck() {
        const running = Array.from(this.executions.values()).filter(e => e.status === 'running').length;
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
    async decomposeGoal(goal, context = {}) {
        // Analyze goal and create subtasks
        const subtasks = this.analyzeGoal(goal, context);
        // Build execution plan respecting dependencies
        const executionPlan = this.topologicalSort(subtasks);
        return { goal, subtasks, executionPlan };
    }
    /** Simple goal analysis — extracts subtasks from goal description */
    analyzeGoal(goal, context) {
        const subtasks = [];
        const lines = goal.split(/[.\n]/).map(l => l.trim()).filter(Boolean);
        // If goal has explicit steps (numbered or bulleted), use them
        const explicitSteps = lines.filter(l => /^\d+[\.\)]\s/.test(l) || /^[-*]\s/.test(l));
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
        }
        else {
            // Default decomposition: research -> plan -> execute -> verify
            subtasks.push({ id: 'research', name: 'Research & gather info', description: `Research context for: ${goal}`, dependencies: [] }, { id: 'plan', name: 'Create execution plan', description: 'Plan approach and resources', dependencies: ['research'] }, { id: 'execute', name: 'Execute main task', description: goal, dependencies: ['plan'] }, { id: 'verify', name: 'Verify results', description: 'Check that goal is achieved', dependencies: ['execute'] });
        }
        return subtasks;
    }
    /** Topological sort for dependency ordering */
    topologicalSort(subtasks) {
        const inDegree = new Map();
        const adjacency = new Map();
        const ids = subtasks.map(s => s.id);
        for (const id of ids) {
            inDegree.set(id, 0);
            adjacency.set(id, []);
        }
        for (const task of subtasks) {
            for (const dep of task.dependencies) {
                if (adjacency.has(dep)) {
                    adjacency.get(dep).push(task.id);
                    inDegree.set(task.id, (inDegree.get(task.id) ?? 0) + 1);
                }
            }
        }
        const queue = ids.filter(id => (inDegree.get(id) ?? 0) === 0);
        const order = [];
        while (queue.length > 0) {
            const id = queue.shift();
            order.push(id);
            for (const neighbor of adjacency.get(id) ?? []) {
                inDegree.set(neighbor, (inDegree.get(neighbor) ?? 1) - 1);
                if ((inDegree.get(neighbor) ?? 0) === 0)
                    queue.push(neighbor);
            }
        }
        return order;
    }
    // ─── Workflow Management ──────────────────────────────────────────────────
    /** Register a workflow definition */
    registerWorkflow(workflow) {
        const now = Date.now();
        const full = { ...workflow, createdAt: now, updatedAt: now };
        this.workflows.set(workflow.id, full);
        this.ctx.logger.info('Workflow registered', { id: workflow.id, name: workflow.name, steps: workflow.steps.length });
        return full;
    }
    /** Create a workflow from a decomposed goal */
    createWorkflowFromGoal(goalId, decomposed) {
        const steps = decomposed.subtasks.map(task => ({
            id: task.id,
            name: task.name,
            type: 'agent',
            config: { description: task.description, goal: decomposed.goal },
            dependencies: task.dependencies,
            nextSteps: [],
            timeout: this.config.defaultTimeout,
        }));
        // Set nextSteps based on execution plan order
        for (let i = 0; i < decomposed.executionPlan.length - 1; i++) {
            const step = steps.find(s => s.id === decomposed.executionPlan[i]);
            if (step)
                step.nextSteps = [decomposed.executionPlan[i + 1]];
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
    registerStepHandler(type, handler) {
        this.stepHandlers.set(type, handler);
    }
    /** Register a progress listener */
    onProgress(listener) {
        this.progressListeners.push(listener);
    }
    // ─── Execution ────────────────────────────────────────────────────────────
    /** Execute a workflow */
    async execute(workflowId, variables = {}) {
        if (this.runningCount >= this.config.maxConcurrent)
            throw new Error('Maximum concurrent executions reached');
        const workflow = this.workflows.get(workflowId);
        if (!workflow)
            throw new Error(`Workflow not found: ${workflowId}`);
        const executionId = `exec_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
        const execution = {
            id: executionId, workflowId, status: 'running', currentStep: workflow.entryStep,
            completedSteps: [], failedSteps: [],
            variables: { ...workflow.variables, ...variables },
            stepResults: new Map(), progress: 0, startedAt: Date.now(),
        };
        this.executions.set(executionId, execution);
        this.runningCount++;
        // Run asynchronously with dependency awareness
        this.runExecution(execution, workflow).catch(err => {
            this.ctx.logger.error('Workflow execution error', { executionId, error: err instanceof Error ? err.message : String(err) });
        });
        return execution;
    }
    /** Run execution with dependency-aware scheduling */
    async runExecution(execution, workflow) {
        const completed = new Set();
        const failed = new Set();
        const totalSteps = workflow.steps.length;
        try {
            while (execution.status === 'running' && completed.size < totalSteps) {
                // Find steps ready to run (dependencies satisfied)
                const readySteps = workflow.steps.filter(step => !completed.has(step.id) && !failed.has(step.id) &&
                    step.dependencies.every(d => completed.has(d)));
                if (readySteps.length === 0) {
                    // Check if we're stuck (remaining steps have failed dependencies)
                    const remaining = workflow.steps.filter(s => !completed.has(s.id) && !failed.has(s.id));
                    if (remaining.length > 0) {
                        execution.status = 'failed';
                        execution.error = 'Deadlock: remaining steps have unsatisfied dependencies';
                    }
                    break;
                }
                // Run ready steps (parallel if multiple)
                await Promise.all(readySteps.map(async (step) => {
                    execution.currentStep = step.id;
                    this.emitProgress(execution);
                    try {
                        const handler = this.stepHandlers.get(step.type);
                        if (handler) {
                            const result = await handler(step, execution.variables, execution);
                            execution.stepResults.set(step.id, result);
                            // Handle condition branching
                            if (step.type === 'condition' && result && typeof result === 'object') {
                                const r = result;
                                if (r.nextStep)
                                    step.nextSteps = [r.nextStep];
                            }
                        }
                        completed.add(step.id);
                        execution.completedSteps.push(step.id);
                    }
                    catch (err) {
                        failed.add(step.id);
                        execution.failedSteps.push(step.id);
                        this.ctx.logger.error('Step failed', { step: step.name, error: err instanceof Error ? err.message : String(err) });
                        if (step.errorStep) {
                            // Execute error handler step
                            const errorStep = workflow.steps.find(s => s.id === step.errorStep);
                            if (errorStep) {
                                try {
                                    const handler = this.stepHandlers.get(errorStep.type);
                                    if (handler)
                                        await handler(errorStep, execution.variables, execution);
                                }
                                catch { }
                            }
                        }
                    }
                    // Update progress
                    execution.progress = Math.round((completed.size / totalSteps) * 100);
                    this.emitProgress(execution);
                }));
            }
            if (execution.status === 'running') {
                execution.status = completed.size === totalSteps ? 'completed' : 'failed';
            }
        }
        finally {
            execution.completedAt = Date.now();
            this.runningCount--;
            this.emitProgress(execution);
        }
    }
    /** Emit progress to listeners */
    emitProgress(exec) {
        for (const listener of this.progressListeners) {
            try {
                listener(exec);
            }
            catch { }
        }
    }
    /** Get execution status */
    getExecution(executionId) {
        return this.executions.get(executionId);
    }
    /** Pause execution */
    pauseExecution(executionId) {
        const exec = this.executions.get(executionId);
        if (exec?.status === 'running') {
            exec.status = 'paused';
            return true;
        }
        return false;
    }
    /** Resume paused execution */
    async resumeExecution(executionId) {
        const exec = this.executions.get(executionId);
        if (exec?.status === 'paused') {
            exec.status = 'running';
            const workflow = this.workflows.get(exec.workflowId);
            if (workflow)
                this.runExecution(exec, workflow);
            return true;
        }
        return false;
    }
    /** Checkpoint current execution state */
    checkpointExecution(executionId) {
        const exec = this.executions.get(executionId);
        if (!exec)
            return null;
        return {
            id: exec.id, workflowId: exec.workflowId, status: exec.status,
            currentStep: exec.currentStep, completedSteps: [...exec.completedSteps],
            variables: { ...exec.variables },
            stepResults: Array.from(exec.stepResults.entries()),
            progress: exec.progress, startedAt: exec.startedAt,
        };
    }
    /** Restore execution from checkpoint */
    async restoreFromCheckpoint(data) {
        const workflowId = data.workflowId;
        const workflow = this.workflows.get(workflowId);
        if (!workflow)
            throw new Error(`Workflow not found: ${workflowId}`);
        const execution = {
            id: data.id, workflowId, status: 'paused',
            currentStep: data.currentStep,
            completedSteps: data.completedSteps ?? [],
            failedSteps: [],
            variables: data.variables ?? {},
            stepResults: new Map(data.stepResults),
            progress: data.progress, startedAt: data.startedAt,
            checkpointData: data,
        };
        this.executions.set(execution.id, execution);
        return execution;
    }
}
exports.default = new MeshWorkflowsFeature();
