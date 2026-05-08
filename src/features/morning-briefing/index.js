'use strict';
/**
 * Morning Briefing Feature
 *
 * Generates a daily briefing with calendar events, weather,
 * news, tasks, and personalized insights every morning.
 */
Object.defineProperty(exports, '__esModule', { value: true });
/**
 * Morning Briefing feature — daily personalized morning summary.
 *
 * Aggregates calendar, weather, news, and tasks into a concise
 * morning briefing delivered at a configured time.
 */
class MorningBriefingFeature {
  constructor() {
    this.meta = {
      name: 'morning-briefing',
      version: '0.0.5',
      description: 'Daily personalized morning briefing generator',
      dependencies: [],
    };
    this.config = {
      enabled: false,
      deliveryTime: '08:00',
      timezone: 'UTC',
      includeWeather: true,
      includeCalendar: true,
      includeNews: true,
      includeTasks: true,
      includeInsights: true,
      weatherLocation: 'New York, US',
      newsTopics: ['technology', 'science'],
      maxNewsItems: 5,
    };
    this.handlers = [];
    this.briefingHistory = [];
    this.timer = null;
  }
  async init(config, context) {
    this.ctx = context;
    this.config = { ...this.config, ...config };
  }
  async start() {
    // Schedule daily briefing
    this.scheduleNextBriefing();
    this.ctx.logger.info('Morning Briefing active', {
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
    const lastBriefing = this.briefingHistory[this.briefingHistory.length - 1];
    return {
      healthy: true,
      details: {
        briefingsGenerated: this.briefingHistory.length,
        lastBriefing: lastBriefing?.date ?? 'never',
        scheduledTime: this.config.deliveryTime,
      },
    };
  }
  /** Register a handler for briefing delivery */
  onBriefing(handler) {
    this.handlers.push(handler);
  }
  /** Generate a morning briefing */
  async generateBriefing() {
    const today = new Date().toISOString().split('T')[0];
    const sections = [];
    if (this.config.includeWeather) {
      sections.push(await this.getWeatherSection());
    }
    if (this.config.includeCalendar) {
      sections.push(await this.getCalendarSection());
    }
    if (this.config.includeNews) {
      sections.push(await this.getNewsSection());
    }
    if (this.config.includeTasks) {
      sections.push(await this.getTasksSection());
    }
    if (this.config.includeInsights) {
      sections.push(await this.getInsightsSection());
    }
    sections.sort((a, b) => a.priority - b.priority);
    const briefing = {
      id: `briefing_${Date.now()}`,
      date: today,
      generatedAt: Date.now(),
      sections,
      summary: this.generateSummary(sections),
      delivered: false,
    };
    this.briefingHistory.push(briefing);
    // Deliver to handlers
    for (const handler of this.handlers) {
      try {
        await handler(briefing);
      } catch (err) {
        this.ctx.logger.error('Briefing handler error', {
          error: err instanceof Error ? err.message : String(err),
        });
      }
    }
    briefing.delivered = true;
    return briefing;
  }
  /** Get briefing history */
  getHistory(limit = 7) {
    return this.briefingHistory.slice(-limit);
  }
  /** Schedule next briefing */
  scheduleNextBriefing() {
    const now = new Date();
    const [hours, minutes] = this.config.deliveryTime.split(':').map(Number);
    const target = new Date(now);
    target.setHours(hours, minutes, 0, 0);
    if (target <= now) {
      target.setDate(target.getDate() + 1);
    }
    const delay = target.getTime() - now.getTime();
    this.timer = setTimeout(async () => {
      await this.generateBriefing();
      this.scheduleNextBriefing(); // Reschedule for next day
    }, delay);
  }
  async getWeatherSection() {
    // Would fetch real weather data
    return {
      type: 'weather',
      title: 'Weather',
      priority: 1,
      items: [{ title: this.config.weatherLocation, detail: 'Weather data pending integration' }],
    };
  }
  async getCalendarSection() {
    return {
      type: 'calendar',
      title: "Today's Schedule",
      priority: 2,
      items: [],
    };
  }
  async getNewsSection() {
    return {
      type: 'news',
      title: 'News',
      priority: 3,
      items: [],
    };
  }
  async getTasksSection() {
    return {
      type: 'tasks',
      title: 'Tasks',
      priority: 4,
      items: [],
    };
  }
  async getInsightsSection() {
    return {
      type: 'insights',
      title: 'Insights',
      priority: 5,
      items: [],
    };
  }
  generateSummary(sections) {
    const totalItems = sections.reduce((sum, s) => sum + s.items.length, 0);
    return `Good morning! Your briefing includes ${sections.length} sections with ${totalItems} items.`;
  }
}
exports.default = new MorningBriefingFeature();
