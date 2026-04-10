/**
 * Weather Alerts Feature
 *
 * Weather information retrieval and alerting based on conditions.
 * Uses wttr.in for free weather API (no key required).
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Weather configuration */
export interface WeatherConfig {
    enabled: boolean;
    dbPath: string;
    cacheMinutes: number;
    defaultUnits: 'metric' | 'imperial';
}
/** Weather data */
export interface Weather {
    location: string;
    temperature: number;
    feelsLike: number;
    humidity: number;
    windSpeed: number;
    windDirection: string;
    description: string;
    forecast?: ForecastDay[];
    retrievedAt: number;
}
/** Forecast day */
export interface ForecastDay {
    date: string;
    tempMin: number;
    tempMax: number;
    description: string;
    precipitationChance: number;
}
/** Alert condition */
export interface AlertCondition {
    minTemp?: number;
    maxTemp?: number;
    minHumidity?: number;
    maxHumidity?: number;
    minWind?: number;
    descriptionContains?: string;
    custom?: (weather: Weather) => boolean;
}
/** Stored alert */
export interface WeatherAlert {
    id: string;
    location: string;
    conditionJSON: string;
    createdAt: number;
    lastChecked?: number;
    lastTriggered?: number;
}
/**
 * Weather Alerts feature — weather data and alerting.
 *
 * Fetches weather from wttr.in and evaluates custom conditions.
 * Stores alert definitions in local SQLite.
 */
declare class WeatherAlertsFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private active;
    private weatherCache;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Initialize database */
    private initDb;
    /** Get current weather for a location */
    getWeather(location: string): Promise<Weather>;
    /** Set a weather alert for a location and condition */
    setAlert(location: string, condition: AlertCondition): string;
    /** Get all stored alerts */
    getAlerts(): WeatherAlert[];
    /** Remove an alert */
    removeAlert(id: string): boolean;
    /** Evaluate a condition against weather data */
    private evaluateCondition;
    /** Check all alerts and return triggered ones */
    checkAlerts(): Promise<Array<{
        alert: WeatherAlert;
        weather: Weather;
    }>>;
}
declare const _default: WeatherAlertsFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map