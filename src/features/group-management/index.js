'use strict';
/**
 * Group Management Feature
 *
 * Manages multi-user groups with roles, permissions,
 * shared context, and group-specific settings.
 */
Object.defineProperty(exports, '__esModule', { value: true });
/** Role permissions mapping */
const ROLE_PERMISSIONS = {
  owner: [
    'manage_members',
    'manage_settings',
    'delete_group',
    'manage_context',
    'send_messages',
    'view_history',
  ],
  admin: ['manage_members', 'manage_settings', 'manage_context', 'send_messages', 'view_history'],
  member: ['send_messages', 'view_history'],
  viewer: ['view_history'],
};
/**
 * Group Management feature — multi-user group administration.
 *
 * Manages user groups with role-based permissions, shared context,
 * and group-specific feature configurations.
 */
class GroupManagementFeature {
  constructor() {
    this.meta = {
      name: 'group-management',
      version: '0.0.4',
      description: 'Multi-user group management with roles and permissions',
      dependencies: [],
    };
    this.config = {
      enabled: false,
      maxGroups: 100,
      maxMembersPerGroup: 50,
      defaultRole: 'member',
      allowSelfJoin: false,
    };
    this.groups = new Map();
    this.userGroups = new Map(); // userId -> groupIds
  }
  async init(config, context) {
    this.ctx = context;
    this.config = { ...this.config, ...config };
  }
  async start() {
    this.ctx.logger.info('Group Management active', {
      maxGroups: this.config.maxGroups,
      maxMembers: this.config.maxMembersPerGroup,
    });
  }
  async stop() {
    this.groups.clear();
    this.userGroups.clear();
  }
  async healthCheck() {
    const totalMembers = Array.from(this.groups.values()).reduce(
      (sum, g) => sum + g.members.size,
      0,
    );
    return {
      healthy: true,
      details: {
        groups: this.groups.size,
        totalMembers,
      },
    };
  }
  /** Create a new group */
  createGroup(name, ownerId, ownerUsername, description = '') {
    if (this.groups.size >= this.config.maxGroups) {
      throw new Error('Maximum group limit reached');
    }
    const id = `group_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const group = {
      id,
      name,
      description,
      ownerId,
      members: new Map(),
      settings: {
        isPublic: false,
        allowInvites: true,
        maxMembers: this.config.maxMembersPerGroup,
        messageRetentionDays: 30,
        features: [],
      },
      sharedContext: {},
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };
    // Add owner as first member
    group.members.set(ownerId, {
      userId: ownerId,
      username: ownerUsername,
      role: 'owner',
      joinedAt: Date.now(),
      lastActiveAt: Date.now(),
      permissions: ROLE_PERMISSIONS.owner,
    });
    this.groups.set(id, group);
    this.addUserToGroupIndex(ownerId, id);
    this.ctx.logger.info('Group created', { groupId: id, name, owner: ownerUsername });
    return group;
  }
  /** Add member to group */
  addMember(groupId, userId, username, role = this.config.defaultRole) {
    const group = this.groups.get(groupId);
    if (!group) return false;
    if (group.members.size >= group.settings.maxMembers) {
      return false;
    }
    if (group.members.has(userId)) return false;
    group.members.set(userId, {
      userId,
      username,
      role,
      joinedAt: Date.now(),
      lastActiveAt: Date.now(),
      permissions: ROLE_PERMISSIONS[role],
    });
    group.updatedAt = Date.now();
    this.addUserToGroupIndex(userId, groupId);
    this.ctx.logger.info('Member added', { groupId, userId, role });
    return true;
  }
  /** Remove member from group */
  removeMember(groupId, userId) {
    const group = this.groups.get(groupId);
    if (!group) return false;
    if (userId === group.ownerId) return false; // Can't remove owner
    const removed = group.members.delete(userId);
    if (removed) {
      group.updatedAt = Date.now();
      this.removeUserFromGroupIndex(userId, groupId);
    }
    return removed;
  }
  /** Update member role */
  updateMemberRole(groupId, userId, newRole) {
    const group = this.groups.get(groupId);
    if (!group) return false;
    const member = group.members.get(userId);
    if (!member) return false;
    member.role = newRole;
    member.permissions = ROLE_PERMISSIONS[newRole];
    group.updatedAt = Date.now();
    return true;
  }
  /** Check if user has permission in group */
  hasPermission(groupId, userId, permission) {
    const group = this.groups.get(groupId);
    if (!group) return false;
    const member = group.members.get(userId);
    if (!member) return false;
    return member.permissions.includes(permission);
  }
  /** Get groups for a user */
  getUserGroups(userId) {
    const groupIds = this.userGroups.get(userId);
    if (!groupIds) return [];
    return Array.from(groupIds)
      .map((id) => this.groups.get(id))
      .filter((g) => g !== undefined);
  }
  /** Update group settings */
  updateSettings(groupId, settings) {
    const group = this.groups.get(groupId);
    if (!group) return false;
    group.settings = { ...group.settings, ...settings };
    group.updatedAt = Date.now();
    return true;
  }
  /** Update shared group context */
  updateSharedContext(groupId, key, value) {
    const group = this.groups.get(groupId);
    if (!group) return false;
    group.sharedContext[key] = value;
    group.updatedAt = Date.now();
    return true;
  }
  /** Delete group */
  deleteGroup(groupId, requesterId) {
    const group = this.groups.get(groupId);
    if (!group) return false;
    if (group.ownerId !== requesterId) return false;
    // Clean up user-group indexes
    for (const member of group.members.values()) {
      this.removeUserFromGroupIndex(member.userId, groupId);
    }
    this.groups.delete(groupId);
    this.ctx.logger.info('Group deleted', { groupId });
    return true;
  }
  addUserToGroupIndex(userId, groupId) {
    const groups = this.userGroups.get(userId) ?? new Set();
    groups.add(groupId);
    this.userGroups.set(userId, groups);
  }
  removeUserFromGroupIndex(userId, groupId) {
    const groups = this.userGroups.get(userId);
    if (groups) {
      groups.delete(groupId);
      if (groups.size === 0) {
        this.userGroups.delete(userId);
      }
    }
  }
}
exports.default = new GroupManagementFeature();
