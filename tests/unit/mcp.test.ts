/**
 * Argentum MCP Server Tests
 */

import { describe, test, expect, beforeAll, afterAll } from '@jest/globals';
import { MCPServer, createTool, builtInTools } from '../src/mcp/server';

describe('MCPServer', () => {
  let server: MCPServer;

  beforeAll(() => {
    server = new MCPServer({
      name: 'test-server',
      version: '0.0.5',
      capabilities: {
        tools: true,
        resources: true,
      },
    });
  });

  afterAll(() => {
    // Cleanup
  });

  describe('Server initialization', () => {
    test('should create server with config', () => {
      expect(server).toBeDefined();
      expect(server.config.name).toBe('test-server');
      expect(server.config.version).toBe('0.0.5');
    });

    test('should report capabilities', () => {
      const caps = server.getCapabilities();
      expect(caps.tools).toBe(true);
      expect(caps.resources).toBe(true);
    });

    test('should respond to ping', () => {
      const pong = server.ping();
      expect(pong.pong).toBe(true);
      expect(pong.server).toBe('test-server');
    });
  });

  describe('Tool registration', () => {
    test('should register a tool', () => {
      const tool = createTool(
        'test_tool',
        'A test tool',
        { type: 'object', properties: { input: { type: 'string' } } },
        async ({ input }) => ({ result: input }),
      );

      server.registerTool(tool);
      const tools = server.listTools();
      expect(tools.some((t) => t.name === 'test_tool')).toBe(true);
    });

    test('should register built-in tools', () => {
      for (const tool of Object.values(builtInTools)) {
        server.registerTool(tool);
      }
      const tools = server.listTools();
      expect(tools.length).toBeGreaterThan(0);
    });
  });

  describe('Tool execution', () => {
    test('should execute a simple tool', async () => {
      const tool = createTool(
        'echo',
        'Echoes the input',
        { type: 'object', properties: { message: { type: 'string' } } },
        async ({ message }) => ({ echo: message }),
      );

      server.registerTool(tool);

      const result = await server.handleToolCall({
        name: 'echo',
        input: { message: 'Hello, World!' },
      });

      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.result).toEqual({ echo: 'Hello, World!' });
      }
    });

    test('should handle tool not found', async () => {
      const result = await server.handleToolCall({
        name: 'nonexistent_tool',
        input: {},
      });

      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toContain('not found');
      }
    });

    test('should handle tool execution error', async () => {
      const tool = createTool(
        'failing_tool',
        'A tool that always fails',
        { type: 'object', properties: {} },
        async () => {
          throw new Error('Intentional failure');
        },
      );

      server.registerTool(tool);

      const result = await server.handleToolCall({
        name: 'failing_tool',
        input: {},
      });

      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toBe('Intentional failure');
      }
    });
  });

  describe('Built-in tools', () => {
    test('should have Read tool', () => {
      const tools = server.listTools();
      expect(tools.some((t) => t.name === 'Read')).toBe(true);
    });

    test('should have Write tool', () => {
      const tools = server.listTools();
      expect(tools.some((t) => t.name === 'Write')).toBe(true);
    });

    test('should have Bash tool', () => {
      const tools = server.listTools();
      expect(tools.some((t) => t.name === 'Bash')).toBe(true);
    });
  });
});

describe('createTool helper', () => {
  test('should create a tool with correct structure', () => {
    const tool = createTool('example', 'An example tool', { type: 'object' }, async () => ({
      success: true,
    }));

    expect(tool.name).toBe('example');
    expect(tool.description).toBe('An example tool');
    expect(tool.inputSchema).toEqual({ type: 'object' });
    expect(typeof tool.handler).toBe('function');
  });
});
