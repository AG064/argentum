'use strict';
/**
 * Evening Recap Feature
 *
 * End-of-day summary with accomplishments, pending tasks,
 * tomorrow's preview, and daily metrics.
 */
Object.defineProperty(exports, '__esModule', { value: true });
/**
 * Evening Recap feature — end-of-day summary and reflection.
 *
 * Generates a nightly recap with accomplishments, pending work,
 * tomorrow's preview, and productivity metrics.
 */
class EveningRecapFeature {
  constructor() {
    this.meta = {
      name: 'evening-recap',
      version: '0.0.5',
      description: 'End-of-day summary with accomplishments and metrics',
      dependencies: [],
    };
    this.config = {
      enabled: false,
      deliveryTime: '21:00',
      timezone: 'UTC',
      includeAccomplishments: true,
      includePendingTasks: true,
      includeTomorrowPreview: true,
      includeMetrics: true,
      includeReflection: true,
    };
    this.handlers = [];
    this.recapHistory = [];
    this.timer = null;
  }
  async init(config, context) {
    this.ctx = context;
    this.config = { ...this.config, ...config };
  }
  async start() {
    this.scheduleNextRecap();
    this.ctx.logger.info('Evening Recap active', {
      deliveryTime: this.config.deliveryTime,
      timezone: this.config.timezone,
    });
  }
  async stop() {
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }
  async healthCheck() {
    return {
      healthy: true,
      details: {
        recapsGenerated: this.recapHistory.length,
        scheduledTime: this.config.deliveryTime,
      },
    };
  }
  /** Register a recap delivery handler */
  onRecap(handler) {
    this.handlers.push(handler);
  }
  /** Generate evening recap */
  async generateRecap() {
    const today = new Date().toISOString().split('T')[0];
    const recap = {
      id: `recap_${Date.now()}`,
      date: today,
      generatedAt: Date.now(),
      accomplishments: await this.getAccomplishments(),
      pendingTasks: await this.getPendingTasks(),
      tomorrowPreview: await this.getTomorrowPreview(),
      metrics: await this.getDailyMetrics(),
      reflection: await this.generateReflection(),
      delivered: false,
    };
    this.recapHistory.push(recap);
    for (const handler of this.handlers) {
      try {
        await handler(recap);
      } catch (err) {
        this.ctx.logger.error('Recap handler error', {
          error: err instanceof Error ? err.message : String(err),
        });
      }
    }
    recap.delivered = true;
    return recap;
  }
  /** Get recap history */
  getHistory(limit = 7) {
    return this.recapHistory.slice(-limit);
  }
  /** Record an accomplishment for today */
  recordAccomplishment(description) {
    const today = new Date().toISOString().split('T')[0];
    const todayRecap = this.recapHistory.find((r) => r.date === today);
    if (todayRecap) {
      todayRecap.accomplishments.push(description);
    }
  }
  /** Schedule next recap */
  scheduleNextRecap() {
    const now = new Date();
    const [hours, minutes] = this.config.deliveryTime.split(':').map(Number);
    const target = new Date(now);
    target.setHours(hours, minutes, 0, 0);
    if (target <= now) {
      target.setDate(target.getDate() + 1);
    }
    const delay = target.getTime() - now.getTime();
    this.timer = setTimeout(async () => {
      await this.generateRecap();
      this.scheduleNextRecap();
    }, delay);
  }
  async getAccomplishments() {
    // Would pull from activity logs
    return [];
  }
  async getPendingTasks() {
    return [];
  }
  async getTomorrowPreview() {
    return [];
  }
  async getDailyMetrics() {
    return {
      tasksCompleted: 0,
      tasksPending: 0,
      messagesExchanged: 0,
      featuresUsed: [],
      activeTimeMinutes: 0,
      focusScore: 0,
    };
  }
  async generateReflection() {
    return 'Take a moment to reflect on your day. What went well? What could be improved?';
  }
}
exports.default = new EveningRecapFeature();
