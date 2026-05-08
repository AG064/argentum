import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
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
    defaultRole: string;
}
export interface DelegationRow {
    id: string;
    from_user: string;
    to_user: string;
    task_id: string | null;
    created_at: number;
}
declare class RoleBasedAccessFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private db;
    private ctx;
    private rolesCache;
    private userRolesCache;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Create a new role */
    createRole(name: string, permissions: Permission[]): Promise<Role>;
    /** Get role by ID */
    getRole(roleId: string): Promise<Role | null>;
    /** Get role by name */
    getRoleByName(name: string): Promise<Role | null>;
    /** List all roles */
    listRoles(): Role[];
    /** Delete a role (cannot delete if users assigned) */
    deleteRole(roleId: string): Promise<boolean>;
    /** Assign a role to a user */
    assignRole(userId: string, roleId: string): Promise<void>;
    /** Remove a specific role from a user */
    removeRole(userId: string, roleId: string): Promise<boolean>;
    /** Remove all roles from a user */
    removeAllRoles(userId: string): Promise<number>;
    /** Get all roles for a user (aggregated permissions) */
    getUserRoles(userId: string): Promise<Role[]>;
    /** Check if user has a specific permission */
    checkPermission(userId: string, action: Permission): Promise<boolean>;
    /** Check if user has any of the specified permissions */
    hasAnyPermission(userId: string, actions: Permission[]): Promise<boolean>;
    /** Set manager (parent) for a user in the org hierarchy */
    setManager(userId: string, managerId: string | null, roleId?: string): Promise<void>;
    /** Get direct reports for a manager */
    getReports(managerId: string): Promise<string[]>;
    /** Delegate a task from one user to another (records delegation) */
    delegateTask(fromUser: string, toUser: string, taskId?: string): Promise<string>;
    /** Get delegations for a user */
    getDelegations(userId: string): Promise<DelegationRow[]>;
    private initDatabase;
    private loadRolesFromDb;
    private ensureDefaultRoles;
}
declare const _default: RoleBasedAccessFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map