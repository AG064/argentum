/**
 * Group Management Feature
 *
 * Manages multi-user groups with roles, permissions,
 * shared context, and group-specific settings.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Group management configuration */
export interface GroupManagementConfig {
    enabled: boolean;
    maxGroups: number;
    maxMembersPerGroup: number;
    defaultRole: GroupRole;
    allowSelfJoin: boolean;
}
/** Group roles */
export type GroupRole = 'owner' | 'admin' | 'member' | 'viewer';
/** Group member */
export interface GroupMember {
    userId: string;
    username: string;
    role: GroupRole;
    joinedAt: number;
    lastActiveAt: number;
    permissions: string[];
}
/** Group */
export interface Group {
    id: string;
    name: string;
    description: string;
    ownerId: string;
    members: Map<string, GroupMember>;
    settings: GroupSettings;
    sharedContext: Record<string, unknown>;
    createdAt: number;
    updatedAt: number;
}
/** Group settings */
export interface GroupSettings {
    isPublic: boolean;
    allowInvites: boolean;
    maxMembers: number;
    messageRetentionDays: number;
    features: string[];
}
/**
 * Group Management feature — multi-user group administration.
 *
 * Manages user groups with role-based permissions, shared context,
 * and group-specific feature configurations.
 */
declare class GroupManagementFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private groups;
    private userGroups;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Create a new group */
    createGroup(name: string, ownerId: string, ownerUsername: string, description?: string): Group;
    /** Add member to group */
    addMember(groupId: string, userId: string, username: string, role?: GroupRole): boolean;
    /** Remove member from group */
    removeMember(groupId: string, userId: string): boolean;
    /** Update member role */
    updateMemberRole(groupId: string, userId: string, newRole: GroupRole): boolean;
    /** Check if user has permission in group */
    hasPermission(groupId: string, userId: string, permission: string): boolean;
    /** Get groups for a user */
    getUserGroups(userId: string): Group[];
    /** Update group settings */
    updateSettings(groupId: string, settings: Partial<GroupSettings>): boolean;
    /** Update shared group context */
    updateSharedContext(groupId: string, key: string, value: unknown): boolean;
    /** Delete group */
    deleteGroup(groupId: string, requesterId: string): boolean;
    private addUserToGroupIndex;
    private removeUserFromGroupIndex;
}
declare const _default: GroupManagementFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map