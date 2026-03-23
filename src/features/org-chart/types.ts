/**
 * Org Chart Types
 *
 * Interfaces for organizational hierarchy management.
 * AG-Claw acts as CEO with subagents as team members.
 */

export type AgentType = 'ag-claw' | 'coder' | 'researcher' | 'analyst' | 'foreman' | 'custom';
export type OrgStatus = 'active' | 'paused' | 'terminated';
export type OrgRole = 'CEO' | 'CTO' | 'Engineer' | 'Researcher' | 'Analyst' | 'Foreman' | string;

export interface BudgetConfig {
  monthlyLimit: number;
  perAgentLimits: Record<string, number>;
  alertThreshold: number;
  hardStop: boolean;
  spent: number;
  periodStart: number;
}

export interface OrgNode {
  id: string;
  name: string;
  role: OrgRole;
  parentId?: string;
  agentType: AgentType;
  config: Record<string, unknown>;
  budget?: BudgetConfig;
  status: OrgStatus;
  hiredAt: number;
  notes?: string;
}

export interface OrgChartConfig {
  enabled: boolean;
  dbPath: string;
  ceoId: string;
  defaultBudget: BudgetConfig;
}

export interface TaskAssignment {
  taskId: string;
  agentId: string;
  assignedAt: number;
  priority: number;
  status: 'pending' | 'in_progress' | 'completed' | 'cancelled';
  description: string;
}

export interface OrgStats {
  totalAgents: number;
  activeAgents: number;
  pausedAgents: number;
  byRole: Record<string, number>;
  byType: Record<string, number>;
  totalBudget: number;
  totalSpent: number;
}
