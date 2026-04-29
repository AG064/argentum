import { mkdirSync, existsSync } from 'fs';
import { dirname, resolve } from 'path';

import Database from 'better-sqlite3';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

// ─── Types ───────────────────────────────────────────────────────────────────

export type Permission = 'read' | 'write' | 'execute' | 'admin';

export interface Role {
  id: string;
  name: string;
  permissions: Permission[];
}

export interface UserRole {
  userId: string;
  roleId: string;
  assignedAt: number;
}

export interface RoleBasedAccessConfig {
  enabled: boolean;
  dbPath: string;
  defaultRole: string; // Role assigned to new users automatically
}

// ─── Feature ─────────────────────────────────────────────────────────────────

class RoleBasedAccessFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'role-based-access',
    version: '0.0.4',
    description: 'Manage roles and permissions for users and agents',
    dependencies: [],
  };

  private config: RoleBasedAccessConfig = {
    enabled: false,
    dbPath: './data/role-based-access.db',
    defaultRole: 'viewer',
  };
  private db!: Database.Database;
  private ctx!: FeatureContext;
  private rolesCache: Map<string, Role> = new Map();
  private userRolesCache: Map<string, Set<string>> = new Map(); // userId -> Set of roleIds

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<RoleBasedAccessConfig>) };
    this.initDatabase();
    this.loadRolesFromDb();
  }

  async start(): Promise<void> {
    // Ensure default roles exist
    await this.ensureDefaultRoles();

    this.ctx.logger.info('Role-based access active', {
      roles: this.rolesCache.size,
      defaultRole: this.config.defaultRole,
    });
  }

  async stop(): Promise<void> {
    this.db?.close();
  }

  async healthCheck(): Promise<HealthStatus> {
    const roleCount = this.rolesCache.size;
    const userRoleCount = this.userRolesCache.size;
    const defaultRoleExists = this.rolesCache.has(this.config.defaultRole);

    return {
      healthy: defaultRoleExists,
      message: defaultRoleExists
        ? undefined
        : `Default role "${this.config.defaultRole}" not found`,
      details: {
        roles: roleCount,
        userRoleAssignments: userRoleCount,
      },
    };
  }

  // ─── Role Management ─────────────────────────────────────────────────────

  /** Create a new role */
  async createRole(name: string, permissions: Permission[]): Promise<Role> {
    if (!name || permissions.length === 0) {
      throw new Error('Role name and at least one permission required');
    }

    // Check for duplicate name
    for (const role of this.rolesCache.values()) {
      if (role.name === name) {
        throw new Error(`Role with name "${name}" already exists`);
      }
    }

    const id = `role:${Date.now()}:${Math.random().toString(36).substr(2, 9)}`;
    const role: Role = { id, name, permissions };

    this.db
      .prepare(`INSERT INTO roles (id, name, permissions) VALUES (?, ?, ?)`)
      .run(id, name, JSON.stringify(permissions));

    this.rolesCache.set(id, role);
    this.ctx.logger.info('Role created', { roleId: id, name, permissions });
    return role;
  }

  /** Get role by ID */
  async getRole(roleId: string): Promise<Role | null> {
    return this.rolesCache.get(roleId) ?? null;
  }

  /** Get role by name */
  async getRoleByName(name: string): Promise<Role | null> {
    for (const role of this.rolesCache.values()) {
      if (role.name === name) {
        return role;
      }
    }
    return null;
  }

  /** List all roles */
  listRoles(): Role[] {
    return Array.from(this.rolesCache.values());
  }

  /** Delete a role (cannot delete if users assigned) */
  async deleteRole(roleId: string): Promise<boolean> {
    // Check if any users have this role
    for (const [userId, roles] of this.userRolesCache.entries()) {
      if (roles.has(roleId)) {
        throw new Error(`Cannot delete role: assigned to user ${userId}`);
      }
    }

    const deleted = this.db.prepare('DELETE FROM roles WHERE id = ?').run(roleId);
    if (deleted.changes > 0) {
      this.rolesCache.delete(roleId);
      this.ctx.logger.info('Role deleted', { roleId });
      return true;
    }
    return false;
  }

  // ─── User Role Assignment ─────────────────────────────────────────────────

  /** Assign a role to a user */
  async assignRole(userId: string, roleId: string): Promise<void> {
    // Verify role exists
    if (!this.rolesCache.has(roleId)) {
      throw new Error(`Role not found: ${roleId}`);
    }

    const now = Date.now();
    this.db
      .prepare(`INSERT OR REPLACE INTO user_roles (user_id, role_id, assigned_at) VALUES (?, ?, ?)`)
      .run(userId, roleId, now);

    // Update cache
    let userRoles = this.userRolesCache.get(userId);
    if (!userRoles) {
      userRoles = new Set();
      this.userRolesCache.set(userId, userRoles);
    }
    userRoles.add(roleId);

    this.ctx.logger.debug('Role assigned to user', { userId, roleId });
  }

  /** Remove a specific role from a user */
  async removeRole(userId: string, roleId: string): Promise<boolean> {
    const result = this.db
      .prepare('DELETE FROM user_roles WHERE user_id = ? AND role_id = ?')
      .run(userId, roleId);

    // Update cache
    const userRoles = this.userRolesCache.get(userId);
    if (userRoles) {
      userRoles.delete(roleId);
      if (userRoles.size === 0) {
        this.userRolesCache.delete(userId);
      }
    }

    if (result.changes > 0) {
      this.ctx.logger.debug('Role removed from user', { userId, roleId });
      return true;
    }
    return false;
  }

  /** Remove all roles from a user */
  async removeAllRoles(userId: string): Promise<number> {
    const result = this.db.prepare('DELETE FROM user_roles WHERE user_id = ?').run(userId);
    this.userRolesCache.delete(userId);
    this.ctx.logger.info('All roles removed from user', { userId, count: result.changes });
    return result.changes;
  }

  /** Get all roles for a user (aggregated permissions) */
  async getUserRoles(userId: string): Promise<Role[]> {
    const userRoles = this.userRolesCache.get(userId);
    if (!userRoles) {
      // Check if user has any explicit roles; if not, assign default
      const defaultRole = await this.getRoleByName(this.config.defaultRole);
      if (defaultRole) {
        await this.assignRole(userId, defaultRole.id);
        return [defaultRole];
      }
      return [];
    }

    const roles: Role[] = [];
    for (const roleId of userRoles) {
      const role = this.rolesCache.get(roleId);
      if (role) roles.push(role);
    }
    return roles;
  }

  /** Check if user has a specific permission */
  async checkPermission(userId: string, action: Permission): Promise<boolean> {
    const roles = await this.getUserRoles(userId);
    for (const role of roles) {
      if (role.permissions.includes(action) || role.permissions.includes('admin')) {
        return true;
      }
    }
    return false;
  }

  /** Check if user has any of the specified permissions */
  async hasAnyPermission(userId: string, actions: Permission[]): Promise<boolean> {
    for (const action of actions) {
      if (await this.checkPermission(userId, action)) {
        return true;
      }
    }
    return false;
  }

  // ─── Org / Delegation ─────────────────────────────────────────────────────

  /** Set manager (parent) for a user in the org hierarchy */
  async setManager(userId: string, managerId: string | null, roleId?: string): Promise<void> {
    if (managerId === userId) throw new Error('User cannot be their own manager');
    const _now = Date.now();
    this.db
      .prepare(
        `INSERT OR REPLACE INTO org_hierarchy (user_id, parent_id, role_id) VALUES (?, ?, ?)`,
      )
      .run(userId, managerId, roleId ?? null);
    this.ctx.logger.info('Manager set', { userId, managerId });
  }

  /** Get direct reports for a manager */
  async getReports(managerId: string): Promise<string[]> {
    const rows = this.db
      .prepare('SELECT user_id FROM org_hierarchy WHERE parent_id = ?')
      .all(managerId) as any[];
    return rows.map((r) => r.user_id);
  }

  /** Delegate a task from one user to another (records delegation) */
  async delegateTask(fromUser: string, toUser: string, taskId?: string): Promise<string> {
    const id = `del:${Date.now()}:${Math.random().toString(36).substr(2, 8)}`;
    this.db
      .prepare(
        'INSERT INTO delegations (id, from_user, to_user, task_id, created_at) VALUES (?, ?, ?, ?, ?)',
      )
      .run(id, fromUser, toUser, taskId ?? null, Date.now());
    this.ctx.logger.info('Task delegated', { id, fromUser, toUser, taskId });
    return id;
  }

  /** Get delegations for a user */
  async getDelegations(userId: string): Promise<any[]> {
    const rows = this.db
      .prepare(
        'SELECT * FROM delegations WHERE from_user = ? OR to_user = ? ORDER BY created_at DESC',
      )
      .all(userId, userId) as any[];
    return rows;
  }

  // ─── Database ─────────────────────────────────────────────────────────────

  private initDatabase(): void {
    const fullPath = resolve(this.config.dbPath);
    if (!existsSync(dirname(fullPath))) {
      mkdirSync(dirname(fullPath), { recursive: true });
    }

    this.db = new Database(fullPath);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('synchronous = NORMAL');

    this.db.exec(`
      CREATE TABLE IF NOT EXISTS roles (
        id TEXT PRIMARY KEY,
        name TEXT UNIQUE NOT NULL,
        permissions TEXT NOT NULL
      );

      CREATE TABLE IF NOT EXISTS user_roles (
        user_id TEXT NOT NULL,
        role_id TEXT NOT NULL,
        assigned_at INTEGER NOT NULL,
        PRIMARY KEY (user_id, role_id),
        FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE
      );

      CREATE TABLE IF NOT EXISTS org_hierarchy (
        user_id TEXT PRIMARY KEY,
        parent_id TEXT,
        role_id TEXT,
        FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE SET NULL
      );

      CREATE TABLE IF NOT EXISTS delegations (
        id TEXT PRIMARY KEY,
        from_user TEXT NOT NULL,
        to_user TEXT NOT NULL,
        task_id TEXT,
        created_at INTEGER NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_user_roles_user ON user_roles(user_id);
      CREATE INDEX IF NOT EXISTS idx_org_parent ON org_hierarchy(parent_id);
    `);
  }

  private loadRolesFromDb(): void {
    const rows = this.db.prepare('SELECT * FROM roles').all();
    for (const row of rows as any[]) {
      const role: Role = {
        id: row.id,
        name: row.name,
        permissions: JSON.parse(row.permissions),
      };
      this.rolesCache.set(role.id, role);
    }

    // Load user-role assignments
    const userRoleRows = this.db.prepare('SELECT * FROM user_roles').all();
    for (const row of userRoleRows as any[]) {
      let userRoles = this.userRolesCache.get(row.user_id);
      if (!userRoles) {
        userRoles = new Set();
        this.userRolesCache.set(row.user_id, userRoles);
      }
      userRoles.add(row.role_id);
    }
  }

  private async ensureDefaultRoles(): Promise<void> {
    const defaultRole = await this.getRoleByName(this.config.defaultRole);
    if (!defaultRole) {
      // Create default viewer role with read permission
      await this.createRole(this.config.defaultRole, ['read']);
    }

    // Ensure admin role exists
    const adminRole = await this.getRoleByName('admin');
    if (!adminRole) {
      await this.createRole('admin', ['read', 'write', 'execute', 'admin']);
    }

    // Ensure user role exists
    const userRole = await this.getRoleByName('user');
    if (!userRole) {
      await this.createRole('user', ['read', 'write', 'execute']);
    }

    // Ensure agent role exists
    const agentRole = await this.getRoleByName('agent');
    if (!agentRole) {
      await this.createRole('agent', ['read', 'write', 'execute']);
    }
  }
}

export default new RoleBasedAccessFeature();
