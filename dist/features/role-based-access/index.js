"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
// ─── Feature ─────────────────────────────────────────────────────────────────
class RoleBasedAccessFeature {
    meta = {
        name: 'role-based-access',
        version: '0.1.0',
        description: 'Manage roles and permissions for users and agents',
        dependencies: [],
    };
    config = {
        enabled: false,
        dbPath: './data/role-based-access.db',
        defaultRole: 'viewer',
    };
    db;
    ctx;
    rolesCache = new Map();
    userRolesCache = new Map(); // userId -> Set of roleIds
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
        this.initDatabase();
        this.loadRolesFromDb();
    }
    async start() {
        // Ensure default roles exist
        await this.ensureDefaultRoles();
        this.ctx.logger.info('Role-based access active', {
            roles: this.rolesCache.size,
            defaultRole: this.config.defaultRole,
        });
    }
    async stop() {
        this.db?.close();
    }
    async healthCheck() {
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
    async createRole(name, permissions) {
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
        const role = { id, name, permissions };
        this.db
            .prepare(`INSERT INTO roles (id, name, permissions) VALUES (?, ?, ?)`)
            .run(id, name, JSON.stringify(permissions));
        this.rolesCache.set(id, role);
        this.ctx.logger.info('Role created', { roleId: id, name, permissions });
        return role;
    }
    /** Get role by ID */
    async getRole(roleId) {
        return this.rolesCache.get(roleId) ?? null;
    }
    /** Get role by name */
    async getRoleByName(name) {
        for (const role of this.rolesCache.values()) {
            if (role.name === name) {
                return role;
            }
        }
        return null;
    }
    /** List all roles */
    listRoles() {
        return Array.from(this.rolesCache.values());
    }
    /** Delete a role (cannot delete if users assigned) */
    async deleteRole(roleId) {
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
    async assignRole(userId, roleId) {
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
    async removeRole(userId, roleId) {
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
    async removeAllRoles(userId) {
        const result = this.db.prepare('DELETE FROM user_roles WHERE user_id = ?').run(userId);
        this.userRolesCache.delete(userId);
        this.ctx.logger.info('All roles removed from user', { userId, count: result.changes });
        return result.changes;
    }
    /** Get all roles for a user (aggregated permissions) */
    async getUserRoles(userId) {
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
        const roles = [];
        for (const roleId of userRoles) {
            const role = this.rolesCache.get(roleId);
            if (role)
                roles.push(role);
        }
        return roles;
    }
    /** Check if user has a specific permission */
    async checkPermission(userId, action) {
        const roles = await this.getUserRoles(userId);
        for (const role of roles) {
            if (role.permissions.includes(action) || role.permissions.includes('admin')) {
                return true;
            }
        }
        return false;
    }
    /** Check if user has any of the specified permissions */
    async hasAnyPermission(userId, actions) {
        for (const action of actions) {
            if (await this.checkPermission(userId, action)) {
                return true;
            }
        }
        return false;
    }
    // ─── Org / Delegation ─────────────────────────────────────────────────────
    /** Set manager (parent) for a user in the org hierarchy */
    async setManager(userId, managerId, roleId) {
        if (managerId === userId)
            throw new Error('User cannot be their own manager');
        const _now = Date.now();
        this.db
            .prepare(`INSERT OR REPLACE INTO org_hierarchy (user_id, parent_id, role_id) VALUES (?, ?, ?)`)
            .run(userId, managerId, roleId ?? null);
        this.ctx.logger.info('Manager set', { userId, managerId });
    }
    /** Get direct reports for a manager */
    async getReports(managerId) {
        const rows = this.db
            .prepare('SELECT user_id FROM org_hierarchy WHERE parent_id = ?')
            .all(managerId);
        return rows.map((r) => r.user_id);
    }
    /** Delegate a task from one user to another (records delegation) */
    async delegateTask(fromUser, toUser, taskId) {
        const id = `del:${Date.now()}:${Math.random().toString(36).substr(2, 8)}`;
        this.db
            .prepare('INSERT INTO delegations (id, from_user, to_user, task_id, created_at) VALUES (?, ?, ?, ?, ?)')
            .run(id, fromUser, toUser, taskId ?? null, Date.now());
        this.ctx.logger.info('Task delegated', { id, fromUser, toUser, taskId });
        return id;
    }
    /** Get delegations for a user */
    async getDelegations(userId) {
        const rows = this.db
            .prepare('SELECT * FROM delegations WHERE from_user = ? OR to_user = ? ORDER BY created_at DESC')
            .all(userId, userId);
        return rows;
    }
    // ─── Database ─────────────────────────────────────────────────────────────
    initDatabase() {
        const fullPath = (0, path_1.resolve)(this.config.dbPath);
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(fullPath);
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
    loadRolesFromDb() {
        const rows = this.db.prepare('SELECT * FROM roles').all();
        for (const row of rows) {
            const role = {
                id: row.id,
                name: row.name,
                permissions: JSON.parse(row.permissions),
            };
            this.rolesCache.set(role.id, role);
        }
        // Load user-role assignments
        const userRoleRows = this.db.prepare('SELECT * FROM user_roles').all();
        for (const row of userRoleRows) {
            let userRoles = this.userRolesCache.get(row.user_id);
            if (!userRoles) {
                userRoles = new Set();
                this.userRolesCache.set(row.user_id, userRoles);
            }
            userRoles.add(row.role_id);
        }
    }
    async ensureDefaultRoles() {
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
exports.default = new RoleBasedAccessFeature();
//# sourceMappingURL=index.js.map