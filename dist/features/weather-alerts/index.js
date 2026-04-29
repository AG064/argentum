"use strict";
/**
 * Weather Alerts Feature
 *
 * Weather information retrieval and alerting based on conditions.
 * Uses wttr.in for free weather API (no key required).
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const path_1 = __importDefault(require("path"));
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
/**
 * Weather Alerts feature — weather data and alerting.
 *
 * Fetches weather from wttr.in and evaluates custom conditions.
 * Stores alert definitions in local SQLite.
 */
class WeatherAlertsFeature {
    meta = {
        name: 'weather-alerts',
        version: '0.0.4',
        description: 'Weather data retrieval and alerting via wttr.in',
        dependencies: [],
    };
    config = {
        enabled: false,
        dbPath: path_1.default.join(process.cwd(), 'data', 'weather.db'),
        cacheMinutes: 30,
        defaultUnits: 'metric',
    };
    ctx;
    db;
    active = false;
    weatherCache = new Map();
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
    }
    async start() {
        this.initDb();
        this.active = true;
        this.ctx.logger.info('Weather Alerts started', {
            cacheMinutes: this.config.cacheMinutes,
            defaultUnits: this.config.defaultUnits,
        });
    }
    async stop() {
        this.active = false;
        this.db?.close();
        this.weatherCache.clear();
        this.ctx.logger.info('Weather Alerts stopped');
    }
    async healthCheck() {
        try {
            const count = this.db.prepare('SELECT COUNT(*) as c FROM weather_alerts').get().c;
            return {
                healthy: true,
                message: 'Weather Alerts OK',
                details: { cached: this.weatherCache.size, alerts: count },
            };
        }
        catch (err) {
            return { healthy: false, message: 'Weather Alerts error' };
        }
    }
    /** Initialize database */
    initDb() {
        const dbDir = path_1.default.dirname(this.config.dbPath);
        try {
            const { mkdirSync, existsSync } = require('fs');
            if (!existsSync(dbDir)) {
                mkdirSync(dbDir, { recursive: true });
            }
        }
        catch { }
        this.db = new better_sqlite3_1.default(this.config.dbPath);
        this.db.pragma('journal_mode = WAL');
        this.db.exec(`
      CREATE TABLE IF NOT EXISTS weather_alerts (
        id TEXT PRIMARY KEY,
        location TEXT NOT NULL,
        condition_json TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        last_checked INTEGER,
        last_triggered INTEGER
      );
    `);
        this.db.exec(`CREATE INDEX IF NOT EXISTS idx_alert_location ON weather_alerts(location);`);
    }
    /** Get current weather for a location */
    async getWeather(location) {
        const cacheKey = `${location}_${this.config.defaultUnits}`;
        // Check cache first if recent
        const cached = this.weatherCache.get(cacheKey);
        if (cached && Date.now() - cached.fetched < this.config.cacheMinutes * 60 * 1000) {
            this.ctx.logger.debug('Weather cache hit', { location });
            return cached.data;
        }
        this.ctx.logger.debug('Fetching weather', { location });
        // wttr.in endpoint: format=j1 gives JSON
        const url = `https://wttr.in/${encodeURIComponent(location)}?format=j1&num_of_days=3`;
        const response = await fetch(url);
        if (!response.ok) {
            throw new Error(`Weather fetch failed: ${response.status} ${response.statusText}`);
        }
        const data = (await response.json());
        // Parse wttr.in response
        const current = data.current_condition?.[0];
        if (!current)
            throw new Error('Weather data unavailable');
        const weather = {
            location,
            temperature: parseInt(current.temp_C) || parseInt(current.temp_F),
            feelsLike: parseInt(current.FeelsLikeC) || parseInt(current.FeelsLikeF),
            humidity: parseInt(current.humidity),
            windSpeed: parseInt(current.windspeedKmph),
            windDirection: current.winddir16Point,
            description: current.weatherDesc?.[0]?.value || 'Unknown',
            retrievedAt: Date.now(),
        };
        // Parse forecast
        if (data.weather && data.weather.length > 0) {
            weather.forecast = data.weather.map((day) => ({
                date: day.date,
                tempMin: parseInt(day.mintempC) || parseInt(day.mintempF),
                tempMax: parseInt(day.maxtempC) || parseInt(day.maxtempF),
                description: day.weatherDesc[0].value,
                precipitationChance: parseInt(day.chanceofrain),
            }));
        }
        // Cache it
        this.weatherCache.set(cacheKey, { data: weather, fetched: Date.now() });
        return weather;
    }
    /** Set a weather alert for a location and condition */
    setAlert(location, condition) {
        const id = `alert_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
        const now = Date.now();
        const conditionJSON = JSON.stringify(condition);
        const stmt = this.db.prepare(`
      INSERT INTO weather_alerts (id, location, condition_json, created_at) VALUES (?, ?, ?, ?)
    `);
        stmt.run(id, location, conditionJSON, now);
        this.ctx.logger.info('Weather alert set', { id, location });
        return id;
    }
    /** Get all stored alerts */
    getAlerts() {
        const rows = this.db.prepare('SELECT * FROM weather_alerts').all();
        return rows.map((row) => ({
            id: row.id,
            location: row.location,
            conditionJSON: row.condition_json,
            createdAt: row.created_at,
            lastChecked: row.last_checked,
            lastTriggered: row.last_triggered,
        }));
    }
    /** Remove an alert */
    removeAlert(id) {
        const info = this.db.prepare('DELETE FROM weather_alerts WHERE id = ?').run(id);
        return info.changes > 0;
    }
    /** Evaluate a condition against weather data */
    evaluateCondition(condition, weather) {
        if (condition.minTemp !== undefined && weather.temperature < condition.minTemp) {
            return true;
        }
        if (condition.maxTemp !== undefined && weather.temperature > condition.maxTemp) {
            return true;
        }
        if (condition.minHumidity !== undefined && weather.humidity < condition.minHumidity) {
            return true;
        }
        if (condition.maxHumidity !== undefined && weather.humidity > condition.maxHumidity) {
            return true;
        }
        if (condition.minWind !== undefined && weather.windSpeed < condition.minWind) {
            return true;
        }
        if (condition.descriptionContains &&
            weather.description.toLowerCase().includes(condition.descriptionContains.toLowerCase())) {
            return true;
        }
        if (condition.custom) {
            return condition.custom(weather);
        }
        return false;
    }
    /** Check all alerts and return triggered ones */
    async checkAlerts() {
        const alerts = this.getAlerts();
        const triggered = [];
        for (const alert of alerts) {
            try {
                const weather = await this.getWeather(alert.location);
                const condition = JSON.parse(alert.conditionJSON);
                const matches = this.evaluateCondition(condition, weather);
                if (matches) {
                    triggered.push({ alert, weather });
                    // Update last_triggered timestamp
                    this.db
                        .prepare('UPDATE weather_alerts SET last_triggered = ? WHERE id = ?')
                        .run(Date.now(), alert.id);
                }
                // Update last_checked
                this.db
                    .prepare('UPDATE weather_alerts SET last_checked = ? WHERE id = ?')
                    .run(Date.now(), alert.id);
            }
            catch (err) {
                this.ctx.logger.error('Alert check failed', {
                    alertId: alert.id,
                    location: alert.location,
                    error: err instanceof Error ? err.message : String(err),
                });
            }
        }
        return triggered;
    }
}
exports.default = new WeatherAlertsFeature();
//# sourceMappingURL=index.js.map