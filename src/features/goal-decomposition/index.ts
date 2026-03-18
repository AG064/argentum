/**
 * Goal Decomposition (AGX):
 *
 * Automatically breaks down complex goals into sub-goals and tasks.
 * Tracks progress, supports nested decomposition and priorities.
 */

import { v4 as uuidv4 } from 'uuid';
import type { FeatureModule, FeatureMeta, FeatureContext, HealthStatus } from '../../core/plugin-loader';

// ─── Types ───────────────────────────────────────────────────────────────

export interface GoalNode {
  id: string;
  parentId: string | null;
  title: string;
  description?: string;
  status: 'todo' | 'in-progress' | 'done' | 'blocked';
  priority: number;
  createdAt: number;
  updatedAt: number;
  children: GoalNode[];
}

export interface GoalDecompositionConfig {
  enabled: boolean;
  maxDepth: number;
}

// ─── Feature Core ─────────────────────────────────────────────────────────

const DEFAULT_CONFIG: GoalDecompositionConfig = {
  enabled: false,
  maxDepth: 5,
};

class GoalDecompositionFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'goal-decomposition',
    version: '0.1.0',
    description: 'AGX: Goal decomposition into subgoals and tasks with progress tracking',
    dependencies: [],
  };

  private config: GoalDecompositionConfig = { ...DEFAULT_CONFIG };
  private ctx!: FeatureContext;
  private goalTree: Record<string, GoalNode> = {};

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.config = { ...this.config, ...(config as Partial<GoalDecompositionConfig>) };
    this.ctx = context;
  }

  async start(): Promise<void> { /* Restore state if needed */ }
  async stop(): Promise<void> { /* Persist state if needed */ }
  async healthCheck(): Promise<HealthStatus> { return { healthy: true, details: { goalCount: Object.keys(this.goalTree).length } }; }

  /** Add a goal (top-level or subgoal) */
  addGoal(title: string, description?: string, parentId?: string, priority = 5): GoalNode {
    const id = uuidv4();
    const now = Date.now();
    const node: GoalNode = {
      id,
      parentId: parentId ?? null,
      title,
      description,
      status: 'todo',
      priority,
      createdAt: now,
      updatedAt: now,
      children: [],
    };
    this.goalTree[id] = node;
    if (parentId && this.goalTree[parentId]) {
      this.goalTree[parentId].children.push(node);
    }
    return node;
  }

  /** Decompose goal into tasks, given a prompt (LLM should be used in real impl) */
  decomposeGoal(goalId: string, subgoals: Array<{ title: string; description?: string }>): GoalNode[] {
    if (!this.goalTree[goalId]) throw new Error('Parent goal not found');
    return subgoals.map(sg => this.addGoal(sg.title, sg.description, goalId));
  }

  updateGoal(id: string, patch: Partial<Omit<GoalNode, 'id' | 'children'>>): GoalNode | null {
    const node = this.goalTree[id];
    if (!node) return null;
    Object.assign(node, patch, { updatedAt: Date.now() });
    return node;
  }

  /** Mark goal as done */
  completeGoal(id: string): GoalNode | null {
    return this.updateGoal(id, { status: 'done' });
  }

  /** Get a goal and nested children */
  getGoalTree(id: string): GoalNode | null {
    function cloneWithChildren(node: GoalNode): GoalNode {
      return { ...node, children: node.children.map(cloneWithChildren) };
    }
    const node = this.goalTree[id];
    return node ? cloneWithChildren(node) : null;
  }

  listGoals(parentId?: string): GoalNode[] {
    return Object.values(this.goalTree).filter(n => n.parentId === (parentId ?? null));
  }

  deleteGoal(id: string): boolean {
    const node = this.goalTree[id];
    if (!node) return false;
    // Remove from parent's children
    if (node.parentId && this.goalTree[node.parentId]) {
      this.goalTree[node.parentId].children = this.goalTree[node.parentId].children.filter(c => c.id !== id);
    }
    // Depth-first delete
    function deleteRecursively(n: GoalNode, tree: Record<string, GoalNode>) {
      for (const child of n.children) deleteRecursively(child, tree);
      delete tree[n.id];
    }
    deleteRecursively(node, this.goalTree);
    return true;
  }
}

export default new GoalDecompositionFeature();
