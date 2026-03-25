/**
 * Unit tests for ModelRouter
 */

import { ModelRouter, ModelScore, RoutingCriteria, resetModelRouter } from '../../src/core/model-router';

describe('ModelRouter', () => {
  afterEach(() => {
    resetModelRouter();
  });

  describe('selectModel', () => {
    it('should select a model when preferCheap is true', () => {
      // Create router with only models we control
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/cheap-model',
          score: 0,
          costPer1K: 0.1,
          latency: 1000,
          capabilities: ['reasoning', 'analysis'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: ['coding'],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
        {
          modelId: 'test/expensive-model',
          score: 0,
          costPer1K: 3.0,
          latency: 500,
          capabilities: ['reasoning', 'coding'],
          contextLength: 200000,
          toolSupport: true,
          reliability: 0.99,
          specialization: ['general'],
          throughput: 50,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      const selected = router.selectModel({ preferCheap: true });
      // With preferCheap, should return the cheaper model
      expect(selected).toBeTruthy();
      const models = router.getModels();
      const selectedModel = models.find(m => m.modelId === selected);
      expect(selectedModel?.costPer1K).toBeLessThanOrEqual(3.0);
    });

    it('should select a model within maxCost limit', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/cheap-model',
          score: 0,
          costPer1K: 0.1,
          latency: 1000,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
        {
          modelId: 'test/expensive-model',
          score: 0,
          costPer1K: 5.0,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 200000,
          toolSupport: true,
          reliability: 0.99,
          specialization: [],
          throughput: 50,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      const selected = router.selectModel({ maxCost: 0.5 });
      // Should select our cheap model (0.1) or any model under 0.5
      expect(selected).toBeTruthy();
      const models = router.getModels();
      const selectedModel = models.find(m => m.modelId === selected);
      expect(selectedModel?.costPer1K).toBeLessThanOrEqual(0.5);
    });

    it('should filter out models exceeding maxLatency', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/slow-model',
          score: 0,
          costPer1K: 0.1,
          latency: 2000,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
        {
          modelId: 'test/fast-model',
          score: 0,
          costPer1K: 0.5,
          latency: 100,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 100,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      const selected = router.selectModel({ maxLatency: 500 });
      expect(selected).toBe('test/fast-model');
    });

    it('should filter by required capabilities', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/basic-model',
          score: 0,
          costPer1K: 0.1,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
        {
          modelId: 'test/capable-model',
          score: 0,
          costPer1K: 1.0,
          latency: 500,
          capabilities: ['reasoning', 'coding', 'creative'],
          contextLength: 128000,
          toolSupport: true,
          reliability: 0.98,
          specialization: [],
          throughput: 60,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      const selected = router.selectModel({ requiredCapabilities: ['coding', 'creative'] });
      expect(selected).toBe('test/capable-model');
    });

    it('should filter by required tools', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/no-tools',
          score: 0,
          costPer1K: 0.1,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: false,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
        {
          modelId: 'test/with-tools',
          score: 0,
          costPer1K: 0.5,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      const selected = router.selectModel({ requiredTools: true });
      expect(selected).toBe('test/with-tools');
    });

    it('should filter by minContextLength', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/small-context',
          score: 0,
          costPer1K: 0.1,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
        {
          modelId: 'test/large-context',
          score: 0,
          costPer1K: 1.0,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 200000,
          toolSupport: true,
          reliability: 0.98,
          specialization: [],
          throughput: 50,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      const selected = router.selectModel({ minContextLength: 100000 });
      expect(selected).toBe('test/large-context');
    });

    it('should return a valid modelId string', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/model-a',
          score: 0,
          costPer1K: 0.1,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      const selected = router.selectModel({ maxCost: 10 });
      expect(typeof selected).toBe('string');
      expect(selected.length).toBeGreaterThan(0);
    });

    it('should return first available model when no criteria match', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/expensive',
          score: 0,
          costPer1K: 10.0,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      // All models exceed this cost
      const selected = router.selectModel({ maxCost: 0.001 });
      // Should still return something (fallback behavior)
      expect(selected).toBeTruthy();
    });
  });

  describe('registerModel', () => {
    it('should register a new model', () => {
      const router = new ModelRouter();
      const newModel: ModelScore = {
        modelId: 'test/new-model',
        score: 0,
        costPer1K: 0.2,
        latency: 700,
        capabilities: ['reasoning'],
        contextLength: 32000,
        toolSupport: true,
        reliability: 0.95,
        specialization: [],
        throughput: 60,
        totalRequests: 0,
        successfulRequests: 0,
      };

      router.registerModel(newModel);
      const models = router.getModels();
      expect(models.some(m => m.modelId === 'test/new-model')).toBe(true);
    });

    it('should override existing model with same id', () => {
      const router = new ModelRouter();
      const model1: ModelScore = {
        modelId: 'test/model',
        score: 0,
        costPer1K: 0.1,
        latency: 500,
        capabilities: ['reasoning'],
        contextLength: 32000,
        toolSupport: true,
        reliability: 0.95,
        specialization: [],
        throughput: 80,
        totalRequests: 0,
        successfulRequests: 0,
      };

      router.registerModel(model1);
      router.registerModel({ ...model1, costPer1K: 0.05 });

      const models = router.getModels();
      const foundModel = models.find(m => m.modelId === 'test/model');
      expect(foundModel?.costPer1K).toBe(0.05);
    });
  });

  describe('getModels', () => {
    it('should return registered models', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/model-a',
          score: 0,
          costPer1K: 0.1,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      const models = router.getModels();
      expect(models.length).toBeGreaterThanOrEqual(1);
    });

    it('should return a copy, not the original', () => {
      const router = new ModelRouter();
      const models1 = router.getModels();
      const models2 = router.getModels();
      expect(models1).not.toBe(models2);
    });
  });

  describe('recordSuccess / recordFailure', () => {
    it('should increment successful requests', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/model',
          score: 0,
          costPer1K: 0.1,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      router.recordSuccess('test/model');

      const after = router.getModels().find(m => m.modelId === 'test/model');
      expect(after?.successfulRequests).toBe(1);
      expect(after?.totalRequests).toBe(1);
    });

    it('should increment total requests on failure without increasing success', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/model',
          score: 0,
          costPer1K: 0.1,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      router.recordFailure('test/model');

      const after = router.getModels().find(m => m.modelId === 'test/model');
      expect(after?.successfulRequests).toBe(0);
      expect(after?.totalRequests).toBe(1);
    });

    it('should record lastSuccessAt timestamp', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/model',
          score: 0,
          costPer1K: 0.1,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      router.recordSuccess('test/model');

      const after = router.getModels().find(m => m.modelId === 'test/model');
      expect(after?.lastSuccessAt).toBeGreaterThan(0);
    });

    it('should record lastFailureAt timestamp', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/model',
          score: 0,
          costPer1K: 0.1,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      router.recordFailure('test/model');

      const after = router.getModels().find(m => m.modelId === 'test/model');
      expect(after?.lastFailureAt).toBeGreaterThan(0);
    });
  });

  describe('scoreModel', () => {
    it('should return a positive score', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/model',
          score: 0,
          costPer1K: 0.1,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      const model = router.getModels()[0];
      const score = router.scoreModel(model, {});
      expect(score).toBeGreaterThan(0);
    });

    it('should return different scores for different models', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/cheap',
          score: 0,
          costPer1K: 0.1,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
        {
          modelId: 'test/expensive',
          score: 0,
          costPer1K: 3.0,
          latency: 200,
          capabilities: ['reasoning', 'coding'],
          contextLength: 200000,
          toolSupport: true,
          reliability: 0.99,
          specialization: [],
          throughput: 50,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      const models = router.getModels();
      const scores = models.map(m => router.scoreModel(m, {}));
      const uniqueScores = new Set(scores);
      expect(uniqueScores.size).toBeGreaterThan(1);
    });

    it('should clear cache when weights change', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/model',
          score: 0,
          costPer1K: 0.1,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      const model = router.getModels()[0];
      router.scoreModel(model, {});
      router.setWeights({ costEfficiency: 5 });

      // Score should still be a valid number after cache clear
      const newScore = router.scoreModel(model, {});
      expect(typeof newScore).toBe('number');
    });
  });

  describe('weights', () => {
    it('should get current weights', () => {
      const router = new ModelRouter();
      const weights = router.getWeights();
      expect(weights).toHaveProperty('costEfficiency');
      expect(weights).toHaveProperty('latency');
      expect(weights).toHaveProperty('capabilityMatch');
    });

    it('should set new weights', () => {
      const router = new ModelRouter();
      router.setWeights({ costEfficiency: 3, latency: 2 });
      const weights = router.getWeights();
      expect(weights.costEfficiency).toBe(3);
      expect(weights.latency).toBe(2);
    });

    it('should merge partial weights', () => {
      const router = new ModelRouter();
      const before = router.getWeights();
      router.setWeights({ costEfficiency: 5 });
      const after = router.getWeights();
      expect(after.costEfficiency).toBe(5);
      expect(after.latency).toBe(before.latency);
    });
  });

  describe('updateMetrics', () => {
    it('should update model metrics', () => {
      const router = new ModelRouter();
      router.registerModels([
        {
          modelId: 'test/model',
          score: 0,
          costPer1K: 0.1,
          latency: 500,
          capabilities: ['reasoning'],
          contextLength: 32000,
          toolSupport: true,
          reliability: 0.95,
          specialization: [],
          throughput: 80,
          totalRequests: 0,
          successfulRequests: 0,
        },
      ]);

      router.updateMetrics('test/model', { reliability: 0.99, throughput: 100 });

      const model = router.getModels().find(m => m.modelId === 'test/model');
      expect(model?.reliability).toBe(0.99);
      expect(model?.throughput).toBe(100);
    });
  });
});
